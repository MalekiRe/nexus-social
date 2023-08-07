use std::error::Error;
use std::sync::{Arc, Mutex};
use anyhow::Result;
use eframe::{egui, Frame};
use eframe::emath::Align2;
use egui::{Context, WidgetText};
use egui_toast::{Toast, ToastKind, ToastOptions, Toasts};
use reqwest::Client;
use tokio::runtime::Runtime;
use nexus_common::non_api_structs::UserData;
use nexus_client::client::*;
use nexus_common::Username;

fn main() -> Result<()> {
    let options = eframe::NativeOptions {
        initial_window_size: Some(egui::vec2(320.0, 240.0)),
        ..Default::default()
    };
    let rt = tokio::runtime::Runtime::new().unwrap();
    let _enter = rt.enter();
    eframe::run_native(
        "Nexus Social",
        options,
        Box::new(|_cc| Box::new(MyApp::new(rt))),
    ).unwrap();
    Ok(())
}

struct MyApp {
    username_entry: String,
    username: Username,
    user_data: UserData,
    runtime: Runtime,
    client: Client,
    toasts: Toasts,
}
impl MyApp {
    pub fn new(runtime: Runtime) -> Self {
        Self {
            username_entry: "".to_string(),
            runtime,
            user_data:Default::default(),
            client: Default::default(),
            username: Default::default(),
            toasts: Toasts::new()
                .anchor(Align2::LEFT_TOP, (10.0, 10.0))
                .direction(egui::Direction::TopDown),
        }
    }
}
impl eframe::App for MyApp {
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        let mut add_error = |error: String| {
            self.toasts.add(Toast {
                kind: ToastKind::Error,
                text: WidgetText::from(error),
                options: ToastOptions::default()
                    .duration_in_seconds(3.0)
                    .show_progress(true)
                    .show_icon(true),
            });
        };
        egui::CentralPanel::default().show(ctx, |ui| {
        ui.heading("My egui Application");
        ui.text_edit_singleline(&mut self.username_entry);
        if ui.button("login").clicked() {
            match Username::from(&self.username_entry) {
                None => {
                    add_error(String::from("username did not parse"));
                }
                Some(username) => {
                    self.username = username;
                }
            }
        }
        ui.collapsing("friends", |ui| {
            if ui.button("refresh").clicked() {
                self.runtime.block_on(async {
                    match get_friends(&self.client.clone(), self.username.clone()).await {
                        Ok(friends) => {
                            self.user_data.friends = friends;
                        }
                        Err(error) => add_error(error.to_string()),
                    }
                });
            }
        });
        });
        self.toasts.show(ctx);
    }
}
