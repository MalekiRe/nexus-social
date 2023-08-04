use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, MutexGuard};
use axum::Extension;
use axum::extract::Path;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use nexus_common::{FriendRequest, FriendRequestUuid, Invite, InviteUuid, Username};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Database {
    pub users: HashMap<String, UserData>
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

#[derive(Default)]
pub struct InnerState {
    db: Database,
    reqwest_client: reqwest::Client,
}
#[derive(Clone)]
pub struct State(pub Arc<Mutex<InnerState>>);
impl State {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(InnerState::default())))
    }
    pub fn get(&self) -> MutexGuard<InnerState> {
        self.0.lock().unwrap()
    }
}
impl InnerState {
    pub fn user(&self, user: impl AsRef<str>) -> Result<&UserData, StatusCode> {
        self.db.users.get(user.as_ref()).ok_or(StatusCode::NOT_FOUND)
    }
    pub fn user_mut(&mut self, user: impl AsRef<str>) -> Result<&mut UserData, StatusCode> {
        self.db.users.get_mut(user.as_ref()).ok_or(StatusCode::NOT_FOUND)
    }
    pub fn reqwest_client(&self) -> reqwest::Client {
        self.reqwest_client.clone()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut state = State::new();
    let app = axum::Router::new()
        .route("/", get(root))
        .route("/add-user/:username", get(add_user))
        .route("/:username/private/get/friends", get(client_server::get_friends))
        .route("/:username/private/get/sent-friend-requests", get(client_server::get_sent_friend_requests))
        .route("/:username/private/get/rec-friend-requests", get(client_server::get_rec_friend_requests))
        .route("/:username/private/get/friend-request/:uuid", get(client_server::get_friend_request))
        .route("/:username/private/post/send-friend-request", post(client_server::post_send_friend_request))
        .route("/:username/private/post/accept-friend-request", post(client_server::post_accept_friend_request))
        .route("/:username/private/post/deny-friend-request", post(client_server::post_deny_friend_request))
        .route("/:username/private/post/unfriend", post(client_server::post_unfriend))
        .route("/:username/public/post/send-friend-request", post(server_server::post_send_friend_request))
        .route("/:username/public/post/accept-friend-request", post(server_server::post_accept_friend_request))
        .route("/:username/public/post/deny-friend-request", post(server_server::post_deny_friend_request))
        .route("/:username/friend/post/unfriend", post(server_server::post_unfriend))
        .layer(Extension(state))
        ;
    let mut port = 8000;
    if let Some(p) = env::args().into_iter().collect::<Vec<_>>().get(1) {
        port = p.parse().unwrap();
    }
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
async fn add_user(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse, StatusCode> {
    state.get().db.users.insert(username, UserData::default());
    Ok(())
}
mod client_server {
    use axum::{Extension, Json};
    use axum::extract::Path;
    use axum::response::IntoResponse;
    use reqwest::StatusCode;
    use serde_json::Value;
    use nexus_common::{FriendRequest, FriendRequestUuid, UnfriendRequest, Username};
    use crate::State;

    pub async fn get_friends(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse, StatusCode> {
        serde_json::to_string(&state.get()
            .user(username)?
            .friends
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
    pub async fn get_sent_friend_requests(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse, StatusCode> {
        serde_json::to_string(&state
            .get()
            .user(username)?
            .sent_friend_requests
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
    pub async fn get_rec_friend_requests(Extension(state): Extension<State>, Path(username): Path<String>) -> Result<impl IntoResponse, StatusCode> {
        serde_json::to_string(&state
            .get()
            .user(username)?
            .rec_friend_requests
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
    pub async fn get_friend_request(Extension(state): Extension<State>, Path((username, uuid)): Path<(String, String)>) -> Result<impl IntoResponse, StatusCode> {
        serde_json::to_string(&state
            .get()
            .user(username)?
            .friend_requests
                .get(&FriendRequestUuid(uuid))
                .ok_or(StatusCode::NOT_FOUND)?
        ).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
    }
    pub async fn post_send_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse, StatusCode> {
        let friend_request: FriendRequest = serde_json::from_value(payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state.get().user_mut(&username)?.friend_requests.insert(friend_request.uuid.clone(), friend_request.clone());
        state.get().user_mut(&username)?.sent_friend_requests.push(friend_request.uuid.clone());
        eprintln!("A");
        let client = state.get().reqwest_client();
            client
            .post(friend_request.to.to_url().0 + "/public/post/send-friend-request")
            .json(&friend_request)
            .send()
            .await.map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }
    pub async fn post_accept_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse, StatusCode> {
        println!("client_client::post_accept_friend_request");
        let mut friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let user_from = state.get().user_mut(&username)?.friend_requests.remove(&friend_request_uuid).ok_or(StatusCode::NOT_FOUND)?
            .from;
        let pos = state.get().user(&username)?.rec_friend_requests.iter().position(|uuid| uuid.0 == friend_request_uuid.0)
            .ok_or(StatusCode::NOT_FOUND)?;
        state.get().user_mut(&username)?.rec_friend_requests.remove(pos);
        let client = state.get().reqwest_client();
            client
            .post(user_from.to_url().0 + "/public/post/accept-friend-request")
            .json(&friend_request_uuid)
            .send()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state.get().user_mut(username)?.friends.push(user_from);
        Ok(())
    }
    pub async fn post_deny_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse, StatusCode> {
        println!("client_client::post_deny_friend_request");
        let mut friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let user_from = state.get().user_mut(&username)?.friend_requests.remove(&friend_request_uuid).ok_or(StatusCode::NOT_FOUND)?
            .from;
        let pos = state.get().user(&username)?.rec_friend_requests.iter().position(|uuid| uuid.0 == friend_request_uuid.0)
            .ok_or(StatusCode::NOT_FOUND)?;
        state.get().user_mut(&username)?.rec_friend_requests.remove(pos);
        let client = state.get().reqwest_client();
        client
            .post(user_from.to_url().0 + "/public/post/deny-friend-request")
            .json(&friend_request_uuid)
            .send()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }
    pub async fn post_unfriend(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse, StatusCode> {
        println!("client_client::post_unfriend");
        let unfriend_request: UnfriendRequest = serde_json::from_value(payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state.get().user_mut(&username)?.friends.retain(|f| f.clone() != unfriend_request.to);
        let client = state.get().reqwest_client();
        client
            .post(unfriend_request.to.to_url().0 + "/friend/post/unfriend")
            .json(&unfriend_request)
            .send()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok(())
    }
}

mod server_server {
    use axum::{Extension, Json};
    use axum::extract::Path;
    use axum::response::IntoResponse;
    use reqwest::StatusCode;
    use serde_json::Value;
    use nexus_common::{FriendRequest, FriendRequestUuid, UnfriendRequest, Username};
    use crate::State;

    pub async fn post_send_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse, StatusCode> {
        println!("server_server::post_send_friend_request");
        let friend_request: FriendRequest = serde_json::from_value(payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut state = state.get();
        state.user_mut(&username)?.friend_requests.insert(friend_request.uuid.clone(), friend_request.clone());
        state.user_mut(&username)?.rec_friend_requests.push(friend_request.uuid.clone());
        Ok(())
    }

    pub async fn post_accept_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse, StatusCode> {
        println!("server_server::post_accept_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut state = state.get();
        state.user_mut(&username)?.sent_friend_requests.retain(|f| f.0 != friend_request_uuid.0);
        let friend_request = state.user_mut(&username)?.friend_requests.remove(&friend_request_uuid).ok_or(StatusCode::NOT_FOUND)?;
        state.user_mut(username)?.friends.push(friend_request.to);
        Ok(())
    }

    pub async fn post_deny_friend_request(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse, StatusCode> {
        println!("server_server::post_deny_friend_request");
        let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let mut state = state.get();
        state.user_mut(&username)?.sent_friend_requests.retain(|f| f.0 != friend_request_uuid.0);
        state.user_mut(&username)?.friend_requests.remove(&friend_request_uuid).ok_or(StatusCode::NOT_FOUND)?;
        Ok(())
    }

    pub async fn post_unfriend(Extension(state): Extension<State>, Path(username): Path<String>, Json(payload): Json<Value>) -> Result<impl IntoResponse, StatusCode> {
        println!("server_server::post_unfriend");
        let unfriend_request: UnfriendRequest = serde_json::from_value(payload).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        state.get().user_mut(&username)?.friends.retain(|f| f.clone() != unfriend_request.from);
        Ok(())
    }
}