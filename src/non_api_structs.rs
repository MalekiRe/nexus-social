use std::collections::{HashMap, HashSet};
use serde::{Deserialize, Serialize};
use crate::{FriendRequest, FriendRequestUuid, Invite, InviteUuid, Username};

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct UserData {
    pub friends: Vec<Username>,
    pub sent_friend_requests: HashSet<FriendRequestUuid>,
    pub rec_friend_requests: HashSet<FriendRequestUuid>,
    pub friend_requests: HashMap<FriendRequestUuid, FriendRequest>,
    pub invites: HashMap<InviteUuid, Invite>,
    pub sent_invites: HashSet<InviteUuid>,
    pub rec_invites: HashSet<InviteUuid>,
}