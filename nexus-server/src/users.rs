use std::collections::HashMap;

use nexus_common::{FriendRequest, FriendRequestUuid, Invite, InviteUuid, Username};
use serde::{Deserialize, Serialize};
use sled::{Tree, Db};

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

#[derive(Clone)]
pub struct Users {
    tree: Tree,
}

impl Users {
    pub fn new(db: &Db) -> Self {
        Self {
            tree: db.open_tree("users").unwrap(),
        }
    }
}
