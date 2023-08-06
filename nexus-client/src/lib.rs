use anyhow::{Context, Result};
use nexus_common::{FriendRequest, FriendRequestUuid, UnfriendRequest, Username};
use reqwest::Response;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::ffi::{c_char, CStr, CString};
use std::process::Child;

pub struct Client {
    inner: reqwest::Client,
}

impl Client {
    pub fn new() -> Self {
        Self {
            inner: reqwest::Client::new(),
        }
    }

    async fn add_user(&self, username: impl AsRef<Username>) -> anyhow::Result<()> {
        let username = username.as_ref();
        let route = format!("http://{}/add-user/{}", username.website, username.username);
        self.inner.get(route).send().await.context("Adding user")?;
        Ok(())
    }

    async fn user_get(&self, username: impl AsRef<Username>, route: &str) -> Result<Response> {
        let route = format!("{}{}", username.as_ref().to_url().0, route);
        let response = self
            .inner
            .get(&route)
            .send()
            .await
            .with_context(|| format!("GET {}", route))?;
        Ok(response)
    }

    async fn user_get_json<T>(&self, username: impl AsRef<Username>, route: &str) -> Result<T>
    where
        T: DeserializeOwned,
    {
        let value = self
            .user_get(username, route)
            .await?
            .json()
            .await
            .with_context(|| format!("GET {}", route))?;
        Ok(value)
    }

    async fn user_post<T>(
        &self,
        username: impl AsRef<Username>,
        route: &str,
        data: &T,
    ) -> Result<()>
    where
        T: Serialize,
    {
        let route = format!("{}{}", username.as_ref().to_url().0, route);
        self.inner
            .post(&route)
            .json(data)
            .send()
            .await
            .with_context(|| format!("POST {}", route))?
            .text()
            .await
            .with_context(|| format!("POST {}", route))?;
        Ok(())
    }

    pub async fn get_friends(&self, username: impl AsRef<Username>) -> Result<Vec<Username>> {
        self.user_get_json(username, "/private/get/friends").await
    }

    pub async fn rec_friend_requests(
        &self,
        username: impl AsRef<Username>,
    ) -> Result<Vec<FriendRequestUuid>> {
        self.user_get_json(username, "/private/get/rec-friend-requests")
            .await
    }

    pub async fn sent_friend_requests(
        &self,
        username: impl AsRef<Username>,
    ) -> Result<Vec<FriendRequestUuid>> {
        self.user_get_json(username, "/private/get/sent-friend-requests")
            .await
    }

    pub async fn get_friend_request(
        &self,
        username: impl AsRef<Username>,
        fuuid: FriendRequestUuid,
    ) -> Result<FriendRequest> {
        let route = format!("/private/get/friend-request/{}", fuuid.0);
        self.user_get_json(username, &route).await
    }

    pub async fn send_friend_request(&self, friend_request: &FriendRequest) -> Result<()> {
        self.user_post(
            &friend_request.from,
            "/private/post/send-friend-request",
            &friend_request,
        )
        .await
    }

    pub async fn accept_friend_request(
        &self,
        username: impl AsRef<Username>,
        fuuid: FriendRequestUuid,
    ) -> Result<()> {
        self.user_post(username, "/private/post/accept-friend-request", &fuuid)
            .await
    }

    pub async fn deny_friend_request(
        &self,
        username: impl AsRef<Username>,
        fuuid: FriendRequestUuid,
    ) -> Result<()> {
        self.user_post(username, "/private/post/deny-friend-request", &fuuid)
            .await
    }

    pub async fn unfriend(
        &self,
        username: impl AsRef<Username>,
        friend: impl AsRef<Username>,
    ) -> Result<()> {
        self.user_post(
            &username,
            "/private/post/unfriend",
            &UnfriendRequest {
                from: username.as_ref().clone(),
                to: friend.as_ref().clone(),
            },
        )
        .await
    }
}

#[test]
fn test() {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let server1 = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("nexus-server")
        .arg("--")
        .arg("8000")
        .spawn()
        .unwrap();
    let server2 = Command::new("cargo")
        .arg("run")
        .arg("-p")
        .arg("nexus-server")
        .arg("--")
        .arg("9000")
        .spawn()
        .unwrap();
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

    client.add_user(&malek).await?;
    client.add_user(&lyuma).await?;

    let fuuid = FriendRequestUuid(String::from("0"));

    let friends = client.get_friends(&malek).await?;
    assert_eq!(friends.len(), 0);

    let friend_request = FriendRequest {
        from: malek.clone(),
        to: lyuma.clone(),
        uuid: fuuid.clone(),
    };

    client.send_friend_request(&friend_request).await?;
    let s = client.sent_friend_requests(&malek).await?;
    assert_eq!(s.len(), 1);
    assert_eq!(s.first().unwrap().0, fuuid.0);
    let s = client.rec_friend_requests(&lyuma).await?;
    assert_eq!(s.len(), 1);
    assert_eq!(s.first().unwrap().0, fuuid.0);
    let friend_request2 = client
        .get_friend_request(&lyuma, s.first().unwrap().clone())
        .await?;
    assert_eq!(friend_request2, friend_request);
    client.accept_friend_request(&lyuma, fuuid.clone()).await?;
    assert_eq!(
        client
            .get_friends(&malek)
            .await?
            .first()
            .with_context(|| "empty")?
            .clone(),
        lyuma
    );
    assert_eq!(
        client
            .get_friends(&lyuma)
            .await?
            .first()
            .with_context(|| "empty")?
            .clone(),
        malek
    );
    client.unfriend(&malek, &lyuma).await?;
    assert_eq!(client.get_friends(&malek).await?.len(), 0);
    assert_eq!(client.get_friends(&lyuma).await?.len(), 0);

    client.send_friend_request(&friend_request).await?;
    client.deny_friend_request(&lyuma, fuuid.clone()).await?;

    assert_eq!(client.get_friends(&malek).await?.len(), 0);
    assert_eq!(client.get_friends(&lyuma).await?.len(), 0);
    assert_eq!(client.sent_friend_requests(&malek).await?.len(), 0);
    assert_eq!(client.rec_friend_requests(&lyuma).await?.len(), 0);

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
