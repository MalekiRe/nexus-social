pub mod non_api_structs;

use serde::{Deserialize, Serialize};

#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Debug, Serialize, Deserialize, Default)]
pub struct Url(pub String);

#[derive(Eq, PartialEq, Ord, PartialOrd, Hash, Clone, Debug, Serialize, Deserialize, Default)]
#[repr(C)]
pub struct Username{ pub username: String, pub website: String}
impl AsRef<Username> for Username {
    fn as_ref(&self) -> &Username {
        &self
    }
}
impl Username {
    pub fn from(string: impl AsRef<str>) -> Option<Self> {
        let string = string.as_ref().to_string();
        Some(Self {
            username: string.split_once('.')?.0.to_string(),
            website: string.split_once('.')?.1.to_string(),
        })
    }
    pub fn to_url(&self) -> Url {
        Url(String::from("http://") + &self.website + "/" + &self.username)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, Eq, PartialEq)]
pub struct Invite {
    pub from: Username,
    pub to: Username,
    pub uuid: InviteUuid,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, Eq, PartialEq)]
pub struct FriendRequest {
    pub from: Username,
    pub to: Username,
    pub uuid: FriendRequestUuid,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default, Eq, PartialEq)]
pub struct UnfriendRequest {
    pub from: Username,
    pub to: Username,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct AvatarMeta {
    format: AvatarFormat,
    link: Url,
    uuid: AvatarUuid,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub enum AvatarFormat {
    #[default]
    Vrm1_0,
    ReadyPlayerMe,
}
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize, Default)]
pub struct InviteUuid(pub String);
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize, Default)]
pub struct FriendRequestUuid(pub String);
#[derive(Eq, PartialEq, Hash, Clone, Debug, Serialize, Deserialize, Default)]
pub struct AvatarUuid(pub String);