use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::changeable::{FriendRequest, User};
use anyhow::Result;

pub mod changeable {
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct User(pub String);

    #[derive(Clone, Debug, Serialize, Deserialize)]
    pub struct FriendRequest(pub String);
}



#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Url(String);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Invite {
    uuid: String,
    game: GameInfo,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Game {
    id: String,
    publish_server: Url,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameInfo {
    game: Game,
    instance: String,
    join_info: String,
    players: Option<Vec<User>>,
}

#[async_trait]
trait UserApi {
    async fn get_username(user: &User) -> Result<String>;
    #[deprecated]
    async fn create_user(username: String) -> Result<User>;
}

#[async_trait]
trait FriendApi {
    async fn send_friend_request(from: &mut User, to: &User) -> Result<()>;
    async fn get_received_friend_requests(user: &User) -> Result<Vec<FriendRequest>>;
    async fn get_sent_friend_requests(user: &User) -> Result<Vec<FriendRequest>>;
    async fn accept_friend_request(user: &mut User, friend_request: &FriendRequest) -> Result<()>;
    async fn deny_friend_request(user: &mut User, friend_request: &FriendRequest) -> Result<()>;
}

#[async_trait]
trait BioApi {
    async fn set_bio(user: &mut User, bio: String) -> Result<()>;
    async fn get_bio(user: &User) -> Result<String>;
}

#[async_trait]
trait InviteApi {
    async fn send_invite(from: &mut User, to: &User, invite: Invite) -> Result<()>;
    async fn received_invites(user: &User) -> Result<Vec<Invite>>;
    async fn sent_invites(user: &User) -> Result<Vec<Invite>>;
    async fn remove_invite(user: &mut User, invite: &Invite) -> Result<()>;
}

#[async_trait]
trait GameApi {
    async fn set_current_game(user: &mut User, game: GameInfo) -> Result<()>;
    async fn get_current_game(user: &User) -> Result<Option<GameInfo>>;
    async fn make_game_public(game_info: &GameInfo);
    async fn get_public_games(game: Game) -> Result<Vec<GameInfo>>;
}

#[async_trait]
trait GameApiExtensions {
    async fn get_hot_games(game: Game) -> Result<Vec<GameInfo>>;
    async fn get_games_type(game: Game, game_type: String) -> Result<Vec<GameInfo>>;
}