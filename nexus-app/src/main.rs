use std::error::Error;
use std::future::Future;
use std::process::{Child, Command};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use anyhow::Result;
use eframe::{egui, Frame};
use eframe::emath::Align2;
use egui::{Context, WidgetText};
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
use reqwest::Client;
use tokio::runtime::Runtime;
use nexus_client::client;
use nexus_common::non_api_structs::UserData;
use nexus_client::client::*;
use nexus_common::{FriendRequest, FriendRequestUuid, UnfriendRequest, Username};

fn main() -> Result<()> {
    let server_runner = ServerRunner::new();
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(620.0, 440.0)),
        ..Default::default()
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _enter = rt.enter();
    rt.block_on(async {
        nexus_client::add_user(&Client::new(), Username::from("malek.localhost:8000").unwrap()).await.unwrap();
        nexus_client::add_user(&Client::new(), Username::from("lyuma.localhost:9000").unwrap()).await.unwrap();
    });
    eframe::run_native(
        "Nexus Social",
        options,
        Box::new(|_cc| Box::new(MyApp::new(rt))),
    ).unwrap();
    Ok(())
}
pub struct ServerRunner(Vec<Child>);
impl ServerRunner {
    pub fn new() -> Self {
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
        thread::sleep(Duration::from_secs(3));
        Self(vec![server1, server2])
    }
}
impl Drop for ServerRunner {
    fn drop(&mut self) {
        for server in &mut self.0 {
            server.kill().unwrap()
        }
    }
}

struct MyApp {
    username_entry: String,
    username: Option<Username>,
    user_data: UserData,
    friend_request_str: String,
    runtime: Option<Runtime>,
    client: Client,
    toasts: Toasts,
}
impl MyApp {
    pub fn new(runtime: Runtime) -> Self {
        Self {
            username_entry: "".to_string(),
            runtime: Some(runtime),
            user_data:Default::default(),
            client: Default::default(),
            username: None,
            toasts: Toasts::new()
                .anchor(Align2::RIGHT_BOTTOM, (10.0, 10.0))
                .direction(egui::Direction::TopDown),
            friend_request_str: "".to_string(),
        }
    }
    fn refresh(&mut self, username: &Username) {
        let runtime = self.runtime.take().unwrap();
        runtime.block_on(async {
            if let Err(error) = self.sync_data(username).await {
                self.add_error(error.to_string());
            }
        });
        self.runtime.replace(runtime);
    }
    async fn sync_data(&mut self, username: &Username) -> Result<()> {
        self.user_data.sent_friend_requests = client::sent_friend_requests(&self.client, username).await?.into_iter().collect();
        self.user_data.rec_friend_requests = client::rec_friend_requests(&self.client, username).await?.into_iter().collect();
        self.user_data.friend_requests.clear();
        for f in self.user_data.sent_friend_requests.clone() {
            self.user_data.friend_requests.insert(f.clone(), client::get_friend_request(&self.client, username, f).await?);
        }
        for f in self.user_data.rec_friend_requests.clone() {
            self.user_data.friend_requests.insert(f.clone(), client::get_friend_request(&self.client, username, f).await?);
        }
        self.user_data.friends = client::get_friends(&self.client, username).await?;
        Ok(())
    }
    fn add_error(&mut self, error: String) {
        self.toasts.add(Toast {
            kind: ToastKind::Error,
            text: WidgetText::from(error),
            options: ToastOptions::default()
                .duration_in_seconds(3.0)
                .show_progress(true)
                .show_icon(true),
        });
    }
}
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        let mut errors = vec![];
        let mut need_refresh = false;
        let runtime = self.runtime.take().unwrap();
        egui::CentralPanel::default().show(ctx, |ui| {
        if let Some(username) = self.username.clone() {
            if ui.button("logout").clicked() {
                self.username.take();
                return;
            }
            if ui.button("refresh").clicked() {
                need_refresh = true;
            }
            ui.text_edit_singleline(&mut self.friend_request_str);
            if ui.button("send friend request").clicked() {
                match Username::from(&self.friend_request_str) {
                    None => self.add_error(String::from("friend request username invalid")),
                    Some(friend_request_username) => {
                        runtime.block_on(async {
                            let friend_request = FriendRequest {
                                from: username.clone(),
                                to: friend_request_username,
                                uuid: FriendRequestUuid(uuid::Uuid::new_v4().to_string()),
                            };
                            match send_friend_request(&self.client.clone(), friend_request).await {
                                Ok(_) => {}
                                Err(error) => self.add_error(error.to_string()),
                            };
                        });
                        need_refresh = true;
                    }
                }
            }
            ui.horizontal(|ui| {
                ui.collapsing("friends", |ui| {
                    for friend in &self.user_data.friends {
                        ui.group(|ui| {
                            if ui.button("unfriend").clicked() {
                                runtime.block_on(async {
                                   if let Err(error) = client::unfriend(&self.client, &username, friend).await {
                                       errors.push(error.to_string());
                                   }
                                });
                                need_refresh = true;
                            }
                            ui.label(format!("{}{}", friend.username, friend.website));
                        });
                    }
                });
                ui.collapsing("sent friend requests", |ui| {
                    for f in &self.user_data.sent_friend_requests {
                        if let Some(f2) = self.user_data.friend_requests.get(f) {
                            ui.group(|ui| {
                                ui.label(format!("{:#?}", f2));
                            });
                        }
                    }
                });
                ui.collapsing("rec friend requests", |ui| {
                   for f in &self.user_data.rec_friend_requests {
                       if let Some(f2) = self.user_data.friend_requests.get(f) {
                           ui.group(|ui| {
                               if ui.button("accept").clicked() {
                                   need_refresh = true;
                                   runtime.block_on(async {
                                      if let Err(error) = client::accept_friend_request(&self.client, &username, f2.uuid.clone()).await {
                                          errors.push(error.to_string());
                                      }
                                   });
                               }
                               if ui.button("deny").clicked() {
                                   need_refresh = true;
                                   runtime.block_on(async {
                                       if let Err(error) = client::deny_friend_request(&self.client, &username, f2.uuid.clone()).await {
                                           errors.push(error.to_string());
                                       }
                                   });
                               }
                               ui.label(format!("{:#?}", f2));
                           });
                       }
                   }
                });
            });
        } else {
            ui.text_edit_singleline(&mut self.username_entry);
            if ui.button("login").clicked() {
                match Username::from(&self.username_entry) {
                    None => {
                        self.add_error(String::from("username did not parse"));
                    }
                    Some(username) => {
                        self.username.replace(username.clone());
                        need_refresh = true;
                    }
                }
            }
        }
        });
        self.runtime.replace(runtime);
        if need_refresh {
            if let Some(username) = self.username.clone() {
                self.refresh(&username);
            }
        }
        for error in errors {
            self.add_error(error)
        }
        self.toasts.show(ctx);
    }
}
