use std::collections::HashMap;

use anyhow::Context;
use axum::{extract::Path, response::IntoResponse, routing::post, Extension, Json, Router};
use nexus_common::{
    FriendRequest, FriendRequestUuid, Invite, InviteUuid, UnfriendRequest, Username,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sled::{
    transaction::{
        ConflictableTransactionError, ConflictableTransactionResult, TransactionError,
        TransactionalTree,
    },
    Db, IVec, Tree,
};

use crate::AppError;

use super::Result;

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

pub struct Transaction<'a> {
    tree: &'a TransactionalTree,
}

impl<'a> Transaction<'a> {
    pub fn get_user(
        &self,
        user: impl AsRef<str>,
    ) -> ConflictableTransactionResult<UserData, AppError> {
        Ok(serde_json::from_slice(
            &self
                .tree
                .get(user.as_ref())?
                .with_context(|| "Error getting user")
                .map_err(AppError::from)?,
        )
        .map_err(AppError::from)?)
    }

    pub fn try_user_mut(
        &self,
        user: impl AsRef<str>,
        func: impl Fn(&mut UserData) -> Result<()>,
    ) -> ConflictableTransactionResult<(), AppError> {
        let key = user.as_ref();
        let data = self.tree.get(key)?;
        let new_data = Self::user_mut_inner(data, &func)
            .map_err(|err| ConflictableTransactionError::Abort(err))?;
        self.tree.insert(key, new_data)?;
        Ok(())
    }

    pub fn user_mut(
        &self,
        user: impl AsRef<str>,
        func: impl Fn(&mut UserData),
    ) -> ConflictableTransactionResult<(), AppError> {
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
}

#[derive(Clone)]
pub struct Users {
    tree: Tree,
    reqwest_client: reqwest::Client,
}

impl Users {
    pub fn new(db: &Db) -> Self {
        Self {
            tree: db.open_tree("users").unwrap(),
            reqwest_client: Default::default(),
        }
    }

    /// Non-atomically retrieves a user by name.
    ///
    /// **DO NOT USE WHEN THIS DATA IS USED TO MODIFY ANOTHER USER.** This has
    /// the potential for leaving the database in an inconsistent state.
    pub fn get_user(&self, user: impl AsRef<str>) -> Result<UserData> {
        Ok(serde_json::from_slice(
            &self
                .tree
                .get(user.as_ref())?
                .with_context(|| "Error getting user")?,
        )?)
    }

    pub fn transaction<T>(
        &self,
        func: impl for<'a> Fn(Transaction<'a>) -> ConflictableTransactionResult<T, AppError>,
    ) -> Result<T> {
        self.tree
            .transaction(|tree| {
                let transaction = Transaction { tree };
                func(transaction)
            })
            .map_err(|err| match err {
                TransactionError::Abort(err) => err,
                TransactionError::Storage(err) => err.into(),
            })
    }

    pub fn route(self) -> Router {
        use axum::routing::get;

        Router::new()
            .route("/add-user/:username", get(add_user))
            .route("/:username/private/get/friends", get(get_friends))
            .route(
                "/:username/private/get/sent-friend-requests",
                get(get_sent_friend_requests),
            )
            .route(
                "/:username/private/get/rec-friend-requests",
                get(get_rec_friend_requests),
            )
            .route(
                "/:username/private/get/friend-request/:uuid",
                get(get_friend_request),
            )
            .route(
                "/:username/private/post/send-friend-request",
                post(post_send_friend_request),
            )
            .route(
                "/:username/private/post/accept-friend-request",
                post(post_accept_friend_request),
            )
            .route(
                "/:username/private/post/deny-friend-request",
                post(post_deny_friend_request),
            )
            .route("/:username/private/post/unfriend", post(post_unfriend))
            .route(
                "/:username/public/post/send-friend-request",
                post(post_send_friend_request),
            )
            .route(
                "/:username/public/post/accept-friend-request",
                post(post_accept_friend_request),
            )
            .route(
                "/:username/public/post/deny-friend-request",
                post(post_deny_friend_request),
            )
            .route("/:username/friend/post/unfriend", post(post_unfriend))
            .layer(Extension(self))
    }
}

async fn add_user(
    Extension(users): Extension<Users>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse> {
    let user = UserData::default();
    let data = serde_json::to_vec(&user)?;
    users.tree.insert(username, data)?;
    Ok(())
}

async fn get_friends(
    Extension(users): Extension<Users>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse> {
    Ok(serde_json::to_string(&users.get_user(username)?.friends)?)
}

pub async fn get_sent_friend_requests(
    Extension(users): Extension<Users>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse> {
    Ok(serde_json::to_string(
        &users.get_user(username)?.sent_friend_requests,
    )?)
}

pub async fn get_rec_friend_requests(
    Extension(users): Extension<Users>,
    Path(username): Path<String>,
) -> Result<impl IntoResponse> {
    Ok(serde_json::to_string(
        &users.get_user(username)?.rec_friend_requests,
    )?)
}

pub async fn get_friend_request(
    Extension(users): Extension<Users>,
    Path((username, uuid)): Path<(String, String)>,
) -> Result<impl IntoResponse> {
    Ok(serde_json::to_string(
        &users
            .get_user(username)?
            .friend_requests
            .get(&FriendRequestUuid(uuid))
            .with_context(|| "FriendRequestUuid not found")?,
    )?)
}

pub async fn post_send_friend_request(
    Extension(users): Extension<Users>,
    Path(username): Path<String>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse> {
    let friend_request: FriendRequest = serde_json::from_value(payload)?;

    users
        .reqwest_client
        .post(friend_request.to.to_url().0 + "/public/post/send-friend-request")
        .json(&friend_request)
        .send()
        .await?;

    users.transaction(|users| {
        users.user_mut(&username, |user| {
            user.friend_requests
                .insert(friend_request.uuid.clone(), friend_request.clone());
        })?;

        users.user_mut(&username, |user| {
            user.sent_friend_requests.push(friend_request.uuid.clone());
        })?;

        Ok(())
    })
}

pub async fn post_accept_friend_request(
    Extension(users): Extension<Users>,
    Path(username): Path<String>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse> {
    println!("client_client::post_accept_friend_request");
    let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;

    let user_from = users.transaction(|users| {
        let user_from = users
            .get_user(&username)?
            .friend_requests
            .remove(&friend_request_uuid)
            .context("FriendRequestUuid not found")
            .map_err(AppError::from)?
            .from;

        users.user_mut(&username, |user| {
            user.friend_requests.remove(&friend_request_uuid).unwrap();
        })?;

        let pos = users
            .get_user(&username)?
            .rec_friend_requests
            .iter()
            .position(|uuid| uuid.0 == friend_request_uuid.0)
            .with_context(|| "FriendRequestUuid not found")
            .map_err(AppError::from)?;

        users.user_mut(&username, |user| {
            user.rec_friend_requests.remove(pos);
        })?;

        users.user_mut(&username, |user| user.friends.push(user_from.clone()))?;

        Ok(user_from)
    })?;

    users
        .reqwest_client
        .post(user_from.to_url().0 + "/public/post/accept-friend-request")
        .json(&friend_request_uuid)
        .send()
        .await?;

    Ok(())
}

pub async fn post_deny_friend_request(
    Extension(users): Extension<Users>,
    Path(username): Path<String>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse> {
    println!("client_client::post_deny_friend_request");
    let friend_request_uuid: FriendRequestUuid = serde_json::from_value(payload)?;

    let user_from = users.transaction(|users| {
        let user_from = users
            .get_user(&username)?
            .friend_requests
            .remove(&friend_request_uuid)
            .context("FriendRequestUuid not found")
            .map_err(AppError::from)?
            .from;

        users.user_mut(&username, |user| {
            user.friend_requests.remove(&friend_request_uuid).unwrap();
        })?;

        let pos = users
            .get_user(&username)?
            .rec_friend_requests
            .iter()
            .position(|uuid| uuid.0 == friend_request_uuid.0)
            .with_context(|| "FriendRequestUuid not found")
            .map_err(AppError::from)?;

        users.user_mut(&username, |user| {
            user.rec_friend_requests.remove(pos);
        })?;

        Ok(user_from)
    })?;

    users
        .reqwest_client
        .post(user_from.to_url().0 + "/public/post/deny-friend-request")
        .json(&friend_request_uuid)
        .send()
        .await?;

    Ok(())
}

pub async fn post_unfriend(
    Extension(users): Extension<Users>,
    Path(username): Path<String>,
    Json(payload): Json<Value>,
) -> Result<impl IntoResponse> {
    println!("client_client::post_unfriend");
    let unfriend_request: UnfriendRequest = serde_json::from_value(payload)?;

    users.transaction(|users| {
        users.user_mut(&username, |user| {
            user.friends.retain(|f| f.clone() != unfriend_request.to);
        })?;

        Ok(())
    })?;

    users
        .reqwest_client
        .post(unfriend_request.to.to_url().0 + "/friend/post/unfriend")
        .json(&unfriend_request)
        .send()
        .await?;

    Ok(())
}
