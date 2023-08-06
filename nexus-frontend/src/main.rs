use std::{sync::Arc, time::Duration};

use druid::{
    im::Vector,
    widget::{Button, CrossAxisAlignment, Flex, Label, List, Scroll, TextBox},
    AppLauncher, Color, Data, ExtEventSink, Lens, Widget, WidgetExt, WindowDesc,
};
use nexus_client::Client;
use nexus_common::{FriendRequestUuid, Username};
use tokio::sync::mpsc;

pub enum Event {
    SendFriendRequest(nexus_common::FriendRequest),
}

type EventSender = Arc<mpsc::UnboundedSender<Event>>;

#[derive(Clone, Data, PartialEq, Eq)]
pub struct Friend {
    pub nick: String,
    pub username: String,
}

#[derive(Clone, Data)]
pub struct FriendRequest {
    pub outgoing: bool,
    pub from: String,
    pub to: String,
}

#[derive(Clone, Data, Lens)]
pub struct State {
    friends: Vector<Friend>,
    friend_request_editor: String,
    friend_requests: Vector<FriendRequest>,
}

fn main() {
    let state = State {
        friends: Vector::new(),
        friend_request_editor: String::new(),
        friend_requests: Vector::new(),
    };

    let (events_tx, events_rx) = mpsc::unbounded_channel();
    let events_tx = Arc::new(events_tx);

    let main_window = WindowDesc::new(ui_builder(events_tx));
    let launcher = AppLauncher::with_window(main_window);

    let sink = launcher.get_external_handle();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    std::thread::spawn(move || {
        runtime.block_on(async_main(sink, events_rx));
    });

    launcher.log_to_console().launch(state).unwrap();
}

async fn async_main(sink: ExtEventSink, mut events: mpsc::UnboundedReceiver<Event>) {
    let client = Arc::new(Client::new());
    let username = Username::from("mars.localhost:8000");

    tokio::spawn({
        let client = client.clone();
        async move {
            while let Some(event) = events.recv().await {
                match event {
                    Event::SendFriendRequest(req) => {
                        client.send_friend_request(&req).await.unwrap()
                    }
                }
            }
        }
    });

    loop {
        let friend_request_uuids = client.sent_friend_requests(&username).await.unwrap();

        let mut friend_requests = Vec::with_capacity(friend_request_uuids.len());
        for req in friend_request_uuids {
            let req = client.get_friend_request(&username, req).await.unwrap();
            let req = FriendRequest {
                outgoing: true,
                from: req.from.username,
                to: req.to.username,
            };
            friend_requests.push(req);
        }

        let friend_requests = Vector::from(friend_requests);

        let friends: Vector<_> = client
            .get_friends(&username)
            .await
            .unwrap()
            .into_iter()
            .map(|friend| Friend {
                nick: friend.username,
                username: friend.website,
            })
            .collect();

        sink.add_idle_callback(move |data: &mut State| {
            data.friend_requests = friend_requests;
            data.friends = friends;
        });

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn ui_builder(events_tx: EventSender) -> impl Widget<State> {
    Flex::row()
        .with_child(friends_list().lens(State::friends))
        .with_default_spacer()
        .with_child(friend_requests(events_tx))
        .cross_axis_alignment(CrossAxisAlignment::Start)
        .must_fill_main_axis(true)
        .padding(10.0)
}

fn friend_requests(events_tx: EventSender) -> impl Widget<State> {
    Flex::column()
        .with_child(Label::new("Friend Requests").with_text_size(36.0))
        .with_default_spacer()
        .with_child(
            Flex::row()
                .with_child(Button::new("Send Friend Request").on_click({
                    let events_tx = events_tx.clone();
                    move |_, data: &mut State, _| {
                        let _ =
                            events_tx.send(Event::SendFriendRequest(nexus_common::FriendRequest {
                                from: Username::from("mars.localhost:8000"),
                                to: Username::from(std::mem::take(&mut data.friend_request_editor)),
                                uuid: FriendRequestUuid("hhhh".to_string()),
                            }));
                    }
                }))
                .with_child(
                    TextBox::new()
                        .with_placeholder("Your new friend's username")
                        .lens(State::friend_request_editor),
                ),
        )
        .with_default_spacer()
        .with_flex_child(
            List::new(friend_request)
                .lens(State::friend_requests)
                .padding(10.0)
                .scroll(),
            1.,
        )
}

fn friend_request() -> impl Widget<FriendRequest> {
    Flex::row()
        .with_child(Label::dynamic(|data: &FriendRequest, _| data.from.clone()))
        .with_child(Label::dynamic(|data: &FriendRequest, _| data.to.clone()))
}

fn friends_list() -> impl Widget<Vector<Friend>> {
    Flex::column()
        .with_child(Label::new("Friends").with_text_size(36.0))
        .with_default_spacer()
        .with_flex_child(List::new(friend).padding(10.0).scroll(), 1.)
}

fn friend() -> impl Widget<Friend> {
    Flex::row()
        .with_child(Label::dynamic(|data: &Friend, _| data.nick.clone()))
        .with_child(
            Label::dynamic(|data: &Friend, _| data.username.clone())
                .with_text_color(druid::theme::DISABLED_TEXT_COLOR),
        )
        .padding(10.0)
        .background(druid::theme::BUTTON_DARK)
        .rounded(5.0)
        .padding(5.0)
}
