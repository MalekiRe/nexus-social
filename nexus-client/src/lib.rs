use std::ffi::{c_char, CStr, CString};
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;
use anyhow::Context;
use reqwest::Client;
use nexus_common::{FriendRequest, FriendRequestUuid, Invite, InviteUuid, Username};
use crate::client::{accept_friend_request, deny_friend_request, get_friend_request, get_friends, get_invite, get_rec_invites, get_sent_invites, rec_friend_requests, remove_invite, send_friend_request, send_invite, sent_friend_requests, unfriend};

pub mod client {
    use reqwest::Client;
    use nexus_common::{FriendRequest, FriendRequestUuid, Invite, InviteUuid, UnfriendRequest, Username};
    use anyhow::Result;
    use futures::StreamExt;
    use crate::username_t;

    pub async fn get_friends(client: &Client, username: impl AsRef<Username>) -> Result<Vec<Username>> {
        Ok(client.get(username.as_ref().to_url().0 + "/private/get/friends")
            .send()
            .await?
            .json::<_>()
            .await?)
    }
    pub async fn send_invite(client: &Client, invite: Invite) -> Result<()> {
        client.post(invite.from.to_url().0 + "/private/post/send-invite")
            .json(&invite)
            .send()
            .await?;
        Ok(())
    }
    pub async fn remove_invite(client: &Client, username: impl AsRef<Username>, invite_uuid: InviteUuid) -> Result<()> {
        client.post(username.as_ref().to_url().0 + "/private/post/remove-invite")
            .json(&invite_uuid)
            .send()
            .await?;
        Ok(())
    }
    pub async fn get_rec_invites(client: &Client, username: impl AsRef<Username>) -> Result<Vec<InviteUuid>> {
        Ok(client.get(username.as_ref().to_url().0 + "/private/get/rec-invites")
            .send()
            .await?
            .json::<_>()
            .await?)
    }
    pub async fn get_sent_invites(client: &Client, username: impl AsRef<Username>) -> Result<Vec<InviteUuid>> {
        Ok(client.get(username.as_ref().to_url().0 + "/private/get/sent-invites")
            .send()
            .await?
            .json::<_>()
            .await?)
    }
    pub async fn get_invite(client: &Client, username: impl AsRef<Username>, invite_uuid: InviteUuid) -> Result<Invite> {
        Ok(client.get(username.as_ref().to_url().0 + "/private/get/invite/" + &invite_uuid.0)
            .send().await?
            .json::<_>()
            .await?)
    }
    pub async fn send_friend_request(client: &Client, friend_request: FriendRequest) -> Result<()> {
        client.post(friend_request.from.to_url().0 + "/private/post/send-friend-request")
            .json(&friend_request)
            .send()
            .await?;
        Ok(())
    }
    pub async fn rec_friend_requests(client: &Client, username: impl AsRef<Username>) -> Result<Vec<FriendRequestUuid>> {
        Ok(client.get(username.as_ref().to_url().0 + "/private/get/rec-friend-requests")
            .send()
            .await?
            .json::<_>()
            .await?)
    }
    pub async fn sent_friend_requests(client: &Client, username: impl AsRef<Username>) -> Result<Vec<FriendRequestUuid>> {
        Ok(client.get(username.as_ref().to_url().0 + "/private/get/sent-friend-requests")
            .send()
            .await?
            .json::<_>()
            .await?)
    }
    pub async fn get_friend_request(client: &Client, username: impl AsRef<Username>, fuuid: FriendRequestUuid) -> Result<FriendRequest> {
        Ok(client.get(username.as_ref().to_url().0 + "/private/get/friend-request/" + &fuuid.0)
            .send()
            .await?
            .json::<_>()
            .await?)
    }
    pub async fn accept_friend_request(client: &Client, username: impl AsRef<Username>, fuuid: FriendRequestUuid) -> Result<()> {
        client
            .post(username.as_ref().to_url().0 + "/private/post/accept-friend-request")
            .json(&fuuid)
            .send()
            .await?;
        Ok(())
    }
    pub async fn deny_friend_request(client: &Client, username: impl AsRef<Username>, fuuid: FriendRequestUuid) -> Result<()> {
        client
            .post(username.as_ref().to_url().0 + "/private/post/deny-friend-request")
            .json(&fuuid)
            .send()
            .await?;
        Ok(())
    }
    pub async fn unfriend(client: &Client, username: impl AsRef<Username>, friend: impl AsRef<Username>) -> Result<()> {
        let username = username.as_ref();
        client
            .post(username.to_url().0 + "/private/post/unfriend")
            .json(&UnfriendRequest{ from: username.clone(), to: friend.as_ref().clone() })
            .send()
            .await?;
        Ok(())
    }
}

async fn add_user(client: &Client, username: impl AsRef<Username>) -> anyhow::Result<()> {
    let username = username.as_ref();
    client.get(String::from("http://") + &username.website + "/add-user/" + &username.username)
        .send().await?;
    Ok(())
}

#[test]
fn test() {
    let server1 = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("nexus-server")
        .arg("--")
        .arg("8000")
        .spawn().unwrap();
    let server2 = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("nexus-server")
        .arg("--")
        .arg("9000")
        .spawn().unwrap();
    thread::sleep(Duration::from_secs(5));
    tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(wrapper(ServerRunner::new(vec![server1, server2])));
}

pub struct ServerRunner(Vec<Child>);
impl ServerRunner {
    pub fn new(servers: Vec<Child>) -> Self {
        Self(servers)
    }
}
impl Drop for ServerRunner {
    fn drop(&mut self) {
        for mut server in &mut self.0 {
            server.kill().unwrap()
        }
    }
}

async fn wrapper(server_runner: ServerRunner) {
    actual_test().await.unwrap();
}

async fn actual_test() -> anyhow::Result<()> {
    let client = Client::new();

    let malek = Username::from("malek.localhost:8000");
    let lyuma = Username::from("lyuma.localhost:9000");

    add_user(&client, &malek).await?;
    add_user(&client, &lyuma).await?;

    let fuuid = FriendRequestUuid(String::from("0"));

    let friends = get_friends(&client, &malek).await?;
    assert_eq!(friends.len(), 0);

    let friend_request = FriendRequest {
        from: malek.clone(),
        to: lyuma.clone(),
        uuid: fuuid.clone(),
    };

    send_friend_request(&client, friend_request.clone()).await?;
    let s = sent_friend_requests(&client, &malek).await?;
    assert_eq!(s.len(), 1);
    assert_eq!(s.first().unwrap().0, fuuid.0);
    let s = rec_friend_requests(&client, &lyuma).await?;
    assert_eq!(s.len(), 1);
    assert_eq!(s.first().unwrap().0, fuuid.0);
    let friend_request2 = get_friend_request(&client, &lyuma, s.first().unwrap().clone()).await?;
    assert_eq!(friend_request2, friend_request);
    accept_friend_request(&client, &lyuma, fuuid.clone()).await?;
    assert_eq!(get_friends(&client, &malek).await?.first().with_context(|| "empty")?.clone(), lyuma);
    assert_eq!(get_friends(&client, &lyuma).await?.first().with_context(|| "empty")?.clone(), malek);

    let invite_uuid = InviteUuid(String::from("1"));

    let invite = Invite {
        from: lyuma.clone(),
        to: malek.clone(),
        uuid: invite_uuid.clone(),
    };

    send_invite(&client, invite.clone()).await?;

    assert_eq!(get_sent_invites(&client, &lyuma).await?.len(), 1);
    assert_eq!(get_rec_invites(&client, &malek).await?.len(), 1);
    assert_eq!(get_invite(&client, &malek, get_rec_invites(&client, &malek).await?.first().unwrap().clone()).await.unwrap(), invite);

    remove_invite(&client, &malek, invite_uuid.clone()).await?;
    remove_invite(&client, &lyuma, invite_uuid.clone()).await?;
    assert_eq!(get_rec_invites(&client, &malek).await?.len(), 0);
    assert_eq!(get_sent_invites(&client, &lyuma).await?.len(), 0);


    unfriend(&client, &malek, &lyuma).await?;
    assert_eq!(get_friends(&client, &malek).await?.len(), 0);
    assert_eq!(get_friends(&client, &lyuma).await?.len(), 0);

    send_friend_request(&client, friend_request.clone()).await?;
    deny_friend_request(&client, &lyuma, fuuid.clone()).await?;

    assert_eq!(get_friends(&client, &malek).await?.len(), 0);
    assert_eq!(get_friends(&client, &lyuma).await?.len(), 0);
    assert_eq!(sent_friend_requests(&client, &malek).await?.len(), 0);
    assert_eq!(rec_friend_requests(&client, &lyuma).await?.len(), 0);

    Ok(())
}

#[repr(C)]
#[allow(non_camel_case_types)]
pub struct username_t {
    username: *mut c_char,
    website: *mut c_char,
}

impl From<Username> for username_t {
    fn from(value: Username) -> Self {
        username_t {
            username: CString::new(value.username).unwrap().into_raw(),
            website: CString::new(value.website).unwrap().into_raw(),
        }
    }
}
impl From<username_t> for Username {
    fn from(value: username_t) -> Self {
        unsafe {
            Username {
                username: CStr::from_ptr(value.username).to_str().unwrap().to_string(),
                website: CStr::from_ptr(value.website).to_str().unwrap().to_string(),
            }
        }
    }
}

pub extern "C" fn client_get_friends(username: username_t, len: *mut usize) -> *mut Username {
    todo!()
    // async fn internal(username: username_t, len: *mut usize) -> *mut Username {
    //     let mut f = client::get_friends(&Client::new(), username.into())
    //         .await.unwrap();
    //     unsafe {
    //         *len = f.len();
    //     }
    //     return f.as_mut_ptr();
    // }
    // futures::executor::block_on(internal(username, len))
}
