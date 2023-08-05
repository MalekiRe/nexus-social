use anyhow::Context;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Extension;
use nexus_common::{FriendRequest, FriendRequestUuid, Invite, InviteUuid, Username};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sled::transaction::{ConflictableTransactionError, TransactionError};
use sled::{Db, IVec};
use std::collections::HashMap;
use std::env;
use std::fs::remove_dir;
use std::net::SocketAddr;

pub type Result<T> = std::result::Result<T, AppError>;

pub struct AppError(anyhow::Error);

// Tell axum how to convert `AppError` into a response.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Something went wrong: {}", self.0),
        )
            .into_response()
    }
}

// Support returning AppErrors as sled transaction errors.
impl From<AppError> for ConflictableTransactionError<AppError> {
    fn from(val: AppError) -> Self {
        ConflictableTransactionError::Abort(val)
    }
}

// This enables using `?` on functions that return `Result<_, anyhow::Error>` to turn them into
// `Result<_, AppError>`. That way you don't need to do that manually.
impl<E> From<E> for AppError
where
    E: Into<anyhow::Error>,
{
    fn from(err: E) -> Self {
        Self(err.into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct UserData {
    pub friends: Vec<Username>,
    pub sent_friend_requests: Vec<FriendRequestUuid>,
    pub rec_friend_requests: Vec<FriendRequestUuid>,
    pub friend_requests: HashMap<FriendRequestUuid, FriendRequest>,
    pub invites: HashMap<InviteUuid, Invite>,
    pub sent_invites: Vec<InviteUuid>,
    pub rec_invites: Vec<InviteUuid>,
}

impl From<UserData> for IVec {
    fn from(value: UserData) -> Self {
        serde_json::to_vec(&value).unwrap().into()
    }
}

#[derive(Clone)]
pub struct State {
    db: Db,
    reqwest_client: reqwest::Client,
}
impl State {
    pub fn new(port: u16) -> Self {
        let sled_path = String::from("sled") + &port.to_string();
        let _ = remove_dir(&sled_path);
        Self {
            db: sled::open(sled_path).unwrap(),
            reqwest_client: Default::default(),
        }
    }
    pub fn user(&self, user: impl AsRef<str>) -> Result<UserData> {
        Ok(serde_json::from_slice(
            &self
                .db
                .get(user.as_ref())?
                .with_context(|| "Error getting user")?,
        )?)
    }
    pub fn try_user_mut(
        &self,
        user: impl AsRef<str>,
        func: impl Fn(&mut UserData) -> Result<()>,
    ) -> Result<()> {
        self.db
            .transaction(|db| {
                let key = user.as_ref();
                let data = db.get(key)?;
                let new_data = Self::user_mut_inner(data, &func)
                    .map_err(|err| ConflictableTransactionError::Abort(err))?;
                db.insert(key, new_data)?;
                Ok(())
            })
            .map_err(|err| match err {
                TransactionError::Abort(err) => err,
                TransactionError::Storage(err) => err.into(),
            })
    }
    pub fn user_mut(&self, user: impl AsRef<str>, func: impl Fn(&mut UserData)) -> Result<()> {
        self.try_user_mut(user, |user| {
            func(user);
            Ok(())
        })
    }
    fn user_mut_inner(
        data: Option<IVec>,
        func: &impl Fn(&mut UserData) -> Result<()>,
    ) -> Result<Vec<u8>> {
        let data = data.context("User not found")?;
        let mut user = serde_json::from_slice(&data).context("Error getting user")?;
        func(&mut user)?;
        let new_data = serde_json::to_vec(&user)?;
        Ok(new_data)
    }
    pub fn reqwest_client(&self) -> reqwest::Client {
        self.reqwest_client.clone()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut port = 8000;
    if let Some(p) = env::args().into_iter().collect::<Vec<_>>().get(1) {
        port = p.parse().unwrap();
    }
    let state = State::new(port);
    let app = axum::Router::new()
        .route("/", get(root))
        .route("/add-user/:username", get(add_user))
        .route(
            "/:username/private/get/friends",
            get(client_server::get_friends),
        )
        .route(
            "/:username/private/get/sent-friend-requests",
            get(client_server::get_sent_friend_requests),
        )
        .route(
            "/:username/private/get/rec-friend-requests",
            get(client_server::get_rec_friend_requests),
        )
        .route(
            "/:username/private/get/friend-request/:uuid",
            get(client_server::get_friend_request),
        )
        .route(
            "/:username/private/post/send-friend-request",
            post(client_server::post_send_friend_request),
        )
        .route(
            "/:username/private/post/accept-friend-request",
            post(client_server::post_accept_friend_request),
        )
        .route(
            "/:username/private/post/deny-friend-request",
            post(client_server::post_deny_friend_request),
        )
        .route(
            "/:username/private/post/unfriend",
            post(client_server::post_unfriend),
        )
        .route(
            "/:username/public/post/send-friend-request",
            post(server_server::post_send_friend_request),
        )
        .route(
            "/:username/public/post/accept-friend-request",
            post(server_server::post_accept_friend_request),
        )
        .route(
            "/:username/public/post/deny-friend-request",
            post(server_server::post_deny_friend_request),
        )
        .route(
            "/:username/friend/post/unfriend",
            post(server_server::post_unfriend),
        )
        .layer(Extension(state));
    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    println!("listening on {}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;
    Ok(())
}

async fn root(Extension(_state): Extension<State>) -> &'static str {
    "Hello World!"
}
async fn add_user(
    Extension(state): Extension<State>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse> {
    state
        .db
        .insert(username, serde_json::to_vec(&UserData::default())?)?;
    Ok(())
}
mod client_server {
    use crate::Result;
    use crate::State;
    use anyhow::Context;
    use axum::extract::Path;
    use axum::response::IntoResponse;
    use axum::{Extension, Json};
    use nexus_common::{FriendRequest, FriendRequestUuid, UnfriendRequest};
    use serde_json::Value;

    pub async fn get_friends(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
    ) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(&state.user(username)?.friends)?)
    }
    pub async fn get_sent_friend_requests(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
    ) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(
            &state.user(username)?.sent_friend_requests,
        )?)
    }
    pub async fn get_rec_friend_requests(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
    ) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(
            &state.user(username)?.rec_friend_requests,
        )?)
    }
    pub async fn get_friend_request(
        Extension(state): Extension<State>,
        Path((username, uuid)): Path<(String, String)>,
    ) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(
            &state
                .user(username)?
                .friend_requests
                .get(&FriendRequestUuid(uuid))
                .with_context(|| "FriendRequestUuid not found")?,
        )?)
    }
    pub async fn post_send_friend_request(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
        Json(payload): Json<Value>,
    ) -> Result<impl IntoResponse> {
        let friend_request: FriendRequest = serde_json::from_value(payload)?;
        state.try_user_mut(&username, |user| {
            Ok({
                user.friend_requests
                    .insert(friend_request.uuid.clone(), friend_request.clone());
            })
        })?;
        state.try_user_mut(&username, |user| {
            Ok({
                user.sent_friend_requests.push(friend_request.uuid.clone());
            })
        })?;
        state
            .reqwest_client
            .post(friend_request.to.to_url().0 + "/public/post/send-friend-request")
            .json(&friend_request)
            .send()
            .await?;
        Ok(())
    }
    pub async fn post_accept_friend_request(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
        Json(payload): Json<Value>,
    ) -> Result<impl IntoResponse> {
        println!("client_client::post_accept_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;
        let user_from = state
            .user(&username)?
            .friend_requests
            .remove(&friend_request_uuid)
            .context("FriendRequestUuid not found")?
            .from;
        state.user_mut(&username, |user| {
            user.friend_requests.remove(&friend_request_uuid).unwrap();
        })?;
        let pos = state
            .user(&username)?
            .rec_friend_requests
            .iter()
            .position(|uuid| uuid.0 == friend_request_uuid.0)
            .with_context(|| "FriendRequestUuid not found")?;
        state.user_mut(&username, |user| {
            user.rec_friend_requests.remove(pos);
        })?;
        state
            .reqwest_client
            .post(user_from.to_url().0 + "/public/post/accept-friend-request")
            .json(&friend_request_uuid)
            .send()
            .await?;
        state.user_mut(username, |user| user.friends.push(user_from.clone()))?;
        Ok(())
    }
    pub async fn post_deny_friend_request(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
        Json(payload): Json<Value>,
    ) -> Result<impl IntoResponse> {
        println!("client_client::post_deny_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;
        let user_from = state
            .user(&username)?
            .friend_requests
            .remove(&friend_request_uuid)
            .context("FriendRequestUuid not found")?
            .from;
        state.user_mut(&username, |user| {
            user.friend_requests.remove(&friend_request_uuid).unwrap();
        })?;
        let pos = state
            .user(&username)?
            .rec_friend_requests
            .iter()
            .position(|uuid| uuid.0 == friend_request_uuid.0)
            .with_context(|| "FriendRequestUuid not found")?;
        state.user_mut(&username, |user| {
            user.rec_friend_requests.remove(pos);
        })?;
        state
            .reqwest_client
            .post(user_from.to_url().0 + "/public/post/deny-friend-request")
            .json(&friend_request_uuid)
            .send()
            .await?;
        Ok(())
    }
    pub async fn post_unfriend(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
        Json(payload): Json<Value>,
    ) -> Result<impl IntoResponse> {
        println!("client_client::post_unfriend");
        let unfriend_request: UnfriendRequest = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| {
            user.friends.retain(|f| f.clone() != unfriend_request.to);
        })?;
        state
            .reqwest_client
            .post(unfriend_request.to.to_url().0 + "/friend/post/unfriend")
            .json(&unfriend_request)
            .send()
            .await?;
        Ok(())
    }
}

mod server_server {
    use crate::Result;
    use crate::State;
    use anyhow::Context;
    use axum::extract::Path;
    use axum::response::IntoResponse;
    use axum::{Extension, Json};
    use nexus_common::{FriendRequest, FriendRequestUuid, UnfriendRequest};
    use serde_json::Value;

    pub async fn post_send_friend_request(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
        Json(payload): Json<Value>,
    ) -> Result<impl IntoResponse> {
        println!("server_server::post_send_friend_request");
        let friend_request: FriendRequest = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| {
            user.friend_requests
                .insert(friend_request.uuid.clone(), friend_request.clone());
        })?;
        state.user_mut(&username, |user| {
            user.rec_friend_requests.push(friend_request.uuid.clone())
        })?;
        Ok(())
    }

    pub async fn post_accept_friend_request(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
        Json(payload): Json<Value>,
    ) -> Result<impl IntoResponse> {
        println!("server_server::post_accept_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| {
            user.sent_friend_requests
                .retain(|f| f.0 != friend_request_uuid.0)
        })?;
        let friend_request = state
            .user(&username)?
            .friend_requests
            .remove(&friend_request_uuid)
            .with_context(|| "FriendRequestUuid did not exist")?;
        state.user_mut(&username, |user| {
            user.friend_requests.remove(&friend_request_uuid).unwrap();
        })?;
        state.user_mut(username, |user| {
            user.friends.push(friend_request.to.clone())
        })?;
        Ok(())
    }

    pub async fn post_deny_friend_request(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
        Json(payload): Json<Value>,
    ) -> Result<impl IntoResponse> {
        println!("server_server::post_deny_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| {
            user.sent_friend_requests
                .retain(|f| f.0 != friend_request_uuid.0)
        })?;
        state.try_user_mut(&username, |user| {
            Ok({
                user.friend_requests
                    .remove(&friend_request_uuid)
                    .with_context(|| "FriendRequestUuid not found")?;
            })
        })?;
        Ok(())
    }

    pub async fn post_unfriend(
        Extension(state): Extension<State>,
        Path(username): Path<String>,
        Json(payload): Json<Value>,
    ) -> Result<impl IntoResponse> {
        println!("server_server::post_unfriend");
        let unfriend_request: UnfriendRequest = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| {
            user.friends.retain(|f| f.clone() != unfriend_request.from)
        })?;
        Ok(())
    }
}
