use std::collections::{HashMap, HashSet};
use std::env;
use std::fs::{remove_dir};
use std::net::SocketAddr;
use axum::Extension;
use axum::extract::Path;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use sled::{Db, IVec};
use nexus_common::{FriendRequest, FriendRequestUuid, Invite, InviteUuid, Username};
use nexus_common::non_api_structs::UserData;
use anyhow::{Context};

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
        Ok(serde_json::from_slice(&self.db.get(user.as_ref())?.with_context(|| "Error getting user")?)?)
    }
    pub fn try_user_mut(&self, user: impl AsRef<str>, func: impl Fn(&mut UserData) -> Result<()> ) -> Result<()> {
        let user = user.as_ref();
        let mut user_data = serde_json::from_slice(&self.db.get(user)?.with_context(|| "Error getting user")?)?;
        func(&mut user_data)?;
        self.db.insert(user, serde_json::to_vec(&user_data)?)?;
        Ok(())
    }
    pub fn user_mut(&self, user: impl AsRef<str>, mut func: impl FnMut(&mut UserData)) -> Result<()> {
        let user = user.as_ref();
        let mut user_data = serde_json::from_slice(&self.db.get(user)?.with_context(|| "Error getting user")?)?;
        func(&mut user_data);
        self.db.insert(user, serde_json::to_vec(&user_data)?)?;
        Ok(())
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
        .route("/:username/private/get/friends", get(client_server::get_friends))
        .route("/:username/private/get/sent-invites", get(client_server::get_sent_invites))
        .route("/:username/private/get/rec-invites", get(client_server::get_rec_invites))
        .route("/:username/private/get/sent-friend-requests", get(client_server::get_sent_friend_requests))
        .route("/:username/private/get/rec-friend-requests", get(client_server::get_rec_friend_requests))
        .route("/:username/private/get/invite/:uuid", get(client_server::get_invite))
        .route("/:username/private/get/friend-request/:uuid", get(client_server::get_friend_request))
        .route("/:username/private/post/send-invite", post(client_server::post_send_invite))
        .route("/:username/private/post/remove-invite", post(client_server::post_remove_invite))
        .route("/:username/private/post/send-friend-request", post(client_server::post_send_friend_request))
        .route("/:username/private/post/accept-friend-request", post(client_server::post_accept_friend_request))
        .route("/:username/private/post/deny-friend-request", post(client_server::post_deny_friend_request))
        .route("/:username/private/post/unfriend", post(client_server::post_unfriend))
        .route("/:username/friend/post/send-invite", post(server_server::post_send_invite))
        .route("/:username/public/post/send-friend-request", post(server_server::post_send_friend_request))
        .route("/:username/public/post/accept-friend-request", post(server_server::post_accept_friend_request))
        .route("/:username/public/post/deny-friend-request", post(server_server::post_deny_friend_request))
        .route("/:username/friend/post/unfriend", post(server_server::post_unfriend))
        .layer(Extension(state))
        ;
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
async fn add_user(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse> {
    state.db.insert(username, serde_json::to_vec(&UserData::default())?)?;
    Ok(())
}
mod client_server {
    use axum::{Extension, Json};
    use axum::extract::Path;
    use axum::response::IntoResponse;
    use serde_json::Value;
    use anyhow::{Context};
    use crate::Result;
    use nexus_common::{FriendRequest, FriendRequestUuid, Invite, InviteUuid, UnfriendRequest};
    use crate::State;

    pub async fn get_friends(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(&state
            .user(username)?
            .friends
        )?)
    }
    pub async fn get_sent_invites(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(&state.user(username)?.sent_invites)?)
    }
    pub async fn get_rec_invites(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(&state.user(username)?.rec_invites)?)
    }
    pub async fn get_sent_friend_requests(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(&state
            .user(username)?
            .sent_friend_requests
        )?)
    }
    pub async fn get_rec_friend_requests(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(&state
            .user(username)?
            .rec_friend_requests
        )?)
    }
    pub async fn get_invite(Extension(state): Extension<State>, Path((username, uuid)): Path<(String, String)>) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(&state.user(username)?.invites.get(&InviteUuid(uuid)).with_context(|| "InviteUuid not found")?)?)
    }
    pub async fn get_friend_request(Extension(state): Extension<State>, Path((username, uuid)): Path<(String, String)>) -> Result<impl IntoResponse> {
        Ok(serde_json::to_string(&state
            .user(username)?
            .friend_requests
                .get(&FriendRequestUuid(uuid)).with_context(|| "FriendRequestUuid not found")?
        )?)
    }
    pub async fn post_send_invite(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        let invite: Invite = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| { user.invites.insert(invite.uuid.clone(), invite.clone());})?;
        state.user_mut(&username, |user| { user.sent_invites.insert(invite.uuid.clone());})?;
        state.reqwest_client
            .post(invite.to.to_url().0 + "/friend/post/send-invite")
            .json(&invite)
            .send()
            .await?;
        Ok(())
    }
    pub async fn post_remove_invite(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("client_client::post_remove_invite");
        let invite_uuid: InviteUuid = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| { user.invites.remove(&invite_uuid); })?;
        state.user_mut(&username, |user| { user.rec_invites.remove(&invite_uuid); })?;
        state.user_mut(&username, |user| { user.sent_invites.remove(&invite_uuid); })?;
        Ok(())
    }

    pub async fn post_send_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        let friend_request: FriendRequest = serde_json::from_value(payload)?;
        state.try_user_mut(&username, |user| Ok({ user.friend_requests.insert(friend_request.uuid.clone(), friend_request.clone()); }))?;
        state.try_user_mut(&username, |user| Ok({ user.sent_friend_requests.insert(friend_request.uuid.clone()); }))?;
        state.reqwest_client
            .post(friend_request.to.to_url().0 + "/public/post/send-friend-request")
            .json(&friend_request)
            .send()
            .await?;
        Ok(())
    }
    pub async fn post_accept_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("client_client::post_accept_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;
        let user_from = state.user(&username)?.friend_requests.remove(&friend_request_uuid)
            .context("FriendRequestUuid not found")?.from;
        state.user_mut(&username, |user| { user.friend_requests.remove(&friend_request_uuid).unwrap(); })?;
        state.user_mut(&username, |user| { user.rec_friend_requests.remove(&friend_request_uuid); })?;
        state.reqwest_client
            .post(user_from.to_url().0 + "/public/post/accept-friend-request")
            .json(&friend_request_uuid)
            .send()
            .await?;
        state.user_mut(username, |user| user.friends.push(user_from.clone()) )?;
        Ok(())
    }
    pub async fn post_deny_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("client_client::post_deny_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;
        let user_from = state.user(&username)?.friend_requests.remove(&friend_request_uuid)
            .context("FriendRequestUuid not found")?.from;
        state.user_mut(&username, |user| { user.friend_requests.remove(&friend_request_uuid).unwrap(); })?;
        state.user_mut(&username, |user| { user.rec_friend_requests.remove(&friend_request_uuid); })?;
        state.reqwest_client
            .post(user_from.to_url().0 + "/public/post/deny-friend-request")
            .json(&friend_request_uuid)
            .send()
            .await?;
        Ok(())
    }
    pub async fn post_unfriend(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("client_client::post_unfriend");
        let unfriend_request: UnfriendRequest = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| { user.friends.retain(|f| f.clone() != unfriend_request.to); })?;
        state.reqwest_client
            .post(unfriend_request.to.to_url().0 + "/friend/post/unfriend")
            .json(&unfriend_request)
            .send()
            .await?;
        Ok(())
    }
}

mod server_server {
    use axum::{Extension, Json};
    use axum::extract::Path;
    use axum::response::IntoResponse;
    use serde_json::Value;
    use nexus_common::{FriendRequest, FriendRequestUuid, Invite, UnfriendRequest};
    use anyhow::{Context};
    use crate::State;
    use crate::Result;

    pub async fn post_send_invite(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("server_server::post_send_invite");
        let invite: Invite = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| { user.invites.insert(invite.uuid.clone(), invite.clone()); })?;
        state.user_mut(&username, |user| { user.rec_invites.insert(invite.uuid.clone()); })?;
        Ok(())
    }

    pub async fn post_send_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("server_server::post_send_friend_request");
        let friend_request: FriendRequest = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| { user.friend_requests.insert(friend_request.uuid.clone(), friend_request.clone()); })?;
        state.user_mut(&username, |user| { user.rec_friend_requests.insert(friend_request.uuid.clone()); })?;
        Ok(())
    }

    pub async fn post_accept_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("server_server::post_accept_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| { user.sent_friend_requests.remove(&friend_request_uuid); })?;
        let friend_request = state.user(&username)?.friend_requests.remove(&friend_request_uuid).with_context(|| "FriendRequestUuid did not exist")?;
        state.user_mut(&username, |user| { user.friend_requests.remove(&friend_request_uuid).unwrap(); })?;
        state.user_mut(username, |user| user.friends.push(friend_request.to.clone()))?;
        Ok(())
    }

    pub async fn post_deny_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("server_server::post_deny_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| user.sent_friend_requests.retain(|f| f.0 != friend_request_uuid.0))?;
        state.try_user_mut(&username, |user| Ok({user.friend_requests.remove(&friend_request_uuid).with_context(|| "FriendRequestUuid not found")?;}))?;
        Ok(())
    }

    pub async fn post_unfriend(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse> {
        println!("server_server::post_unfriend");
        let unfriend_request: UnfriendRequest = serde_json::from_value(payload)?;
        state.user_mut(&username, |user| user.friends.retain(|f| f.clone() != unfriend_request.from))?;
        Ok(())
    }
}