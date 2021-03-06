use yew::{html, Bridge, Bridged, Component, ComponentLink, Html, ShouldRender};
use yew_router::agent::RouteRequest;
use yew_router::prelude::{Route, RouteAgent};
use yew_router::{router::Router, Switch};

use super::browse::{self, Browse, BrowseRoute};
use super::chapter::Chapter;
use super::job;
use super::login::Login;
use super::logout::Logout;

#[derive(Switch, Debug, Clone)]
pub enum AppRoute {
    #[to = "/chapter/{chapter_id}/page/{page}"]
    Chapter(i32, usize),
    #[to = "/login"]
    Login,
    #[to = "/logout"]
    Logout,
    #[to = "{*:path}"]
    Browse(BrowseRoute),
}

pub struct App {
    #[allow(dead_code)]
    link: ComponentLink<Self>,
    router: Box<dyn Bridge<RouteAgent>>,
    route: String,
    worker: Box<dyn Bridge<job::Worker>>,
}

pub enum Msg {
    RouterCallback(Route),
    TokenInvalidorExpired,
    Noop,
}

impl Component for App {
    type Message = Msg;
    type Properties = ();

    fn create(_: Self::Properties, link: ComponentLink<Self>) -> Self {
        let callback = link.callback(|route| Msg::RouterCallback(route));
        let router = RouteAgent::bridge(callback);

        let worker_callback = link.callback(|msg| match msg {
            job::Response::TokenInvalidorExpired => Msg::TokenInvalidorExpired,
            _ => Msg::Noop,
        });
        App {
            link,
            router,
            route: "/".to_string(),
            worker: job::Worker::bridge(worker_callback),
        }
    }

    fn change(&mut self, _props: Self::Properties) -> ShouldRender {
        false
    }

    fn rendered(&mut self, first_render: bool) {
        if first_render {
            self.worker.send(job::Request::ValidateToken);
        }
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::RouterCallback(route) => {
                self.route = route.route;
            }
            Msg::TokenInvalidorExpired => {
                self.router
                    .send(RouteRequest::ChangeRoute(Route::from("/login".to_string())));
            }
            Msg::Noop => {
                return false;
            }
        }
        true
    }

    fn view(&self) -> Html {
        html! {
            <div class="w-full h-screen">
                <Router<AppRoute, ()>
                render = Router::render(|switch: AppRoute| {
                match switch {
                    AppRoute::Chapter(chapter_id, page) => html!{<Chapter chapter_id=chapter_id page=page/>},
                    AppRoute::Login => html!{<Login />},
                    AppRoute::Logout => html!{<Logout />},
                    AppRoute::Browse(route) => {
                        let route: browse::Props = route.into();
                        html!{<Browse with route/>}
                    },
                }}) />
            </div>
        }
    }
}
