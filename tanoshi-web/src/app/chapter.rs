use js_sys;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{HtmlElement, HtmlImageElement};
use yew::prelude::*;
use yew::services::storage::Area;
use yew::services::StorageService;
use yew::utils::{document, window};
use yew::{html, Component, ComponentLink, Html, InputData, Properties, ShouldRender};
use yew_router::{agent::RouteRequest, prelude::*};

use crate::app::{browse::BrowseRoute, job, AppRoute};

use super::component::model::{BackgroundColor, PageRendering, ReadingDirection, SettingParams};
use super::component::{Page, PageList, Spinner, WeakComponentLink};

use tanoshi_lib::manga::{Chapter as ChapterModel, Manga as MangaModel};
use tanoshi_lib::rest::{GetChaptersResponse, GetMangaResponse, GetPagesResponse, HistoryRequest};

#[derive(Clone, Properties)]
pub struct Props {
    pub chapter_id: i32,
    pub page: usize,
}

pub struct Chapter {
    link: ComponentLink<Self>,
    router: Box<dyn Bridge<RouteAgent>>,
    token: String,
    manga_id: i32,
    manga: MangaModel,
    chapter: ChapterModel,
    current_chapter_id: i32,
    current_page: usize,
    chapters: Vec<ChapterModel>,
    previous_chapter_page: usize,
    pages: Vec<String>,
    is_fetching: bool,
    refs: Vec<NodeRef>,
    is_bar_visible: bool,
    settings: SettingParams,
    page_refs: Vec<NodeRef>,
    container_ref: NodeRef,
    closure: Closure<dyn Fn()>,
    is_history_fetching: bool,
    worker: Box<dyn Bridge<job::Worker>>,
    should_fetch: bool,
}

pub enum Msg {
    MangaReady(GetMangaResponse),
    ChapterReady(GetChaptersResponse),
    PagesReady(GetPagesResponse),
    PageForward,
    PagePrevious,
    ToggleBar,
    PageSliderChange(usize),
    RouterCallback,
    SetHistoryRequested,
    ScrollEvent(f64),
    Refresh,
    Noop,
}

impl Component for Chapter {
    type Message = Msg;
    type Properties = Props;

    fn create(props: Self::Properties, link: ComponentLink<Self>) -> Self {
        let callback = link.callback(|_| Msg::RouterCallback);
        let router = RouteAgent::bridge(callback);
        let storage = StorageService::new(Area::Local).unwrap();
        let settings = {
            if let Ok(settings) = storage.restore("settings") {
                serde_json::from_str(settings.as_str()).expect("failed to serialize")
            } else {
                SettingParams::default()
            }
        };
        let token = {
            if let Ok(token) = storage.restore("token") {
                token
            } else {
                "".to_string()
            }
        };

        if settings.background_color == BackgroundColor::Black {
            document()
                .body()
                .expect("document should have a body")
                .dyn_ref::<web_sys::HtmlElement>()
                .unwrap()
                .style()
                .set_property("background-color", "black")
                .expect("failed to set background color");
        }

        let tmp_link = link.clone();
        let closure = Closure::wrap(Box::new(move || {
            let current_scroll = window().scroll_y().expect("error get scroll y")
                + window().inner_height().unwrap().as_f64().unwrap();

            tmp_link.send_message(Msg::ScrollEvent(current_scroll));
        }) as Box<dyn Fn()>);

        let worker_callback = link.callback(|msg| match msg {
            job::Response::MangaFetched(data) => Msg::MangaReady(data),
            job::Response::ChaptersFetched(data) => Msg::ChapterReady(data),
            job::Response::PagesFetched(data) => Msg::PagesReady(data),
            job::Response::HistoryPosted => Msg::SetHistoryRequested,
            _ => Msg::Noop,
        });

        Chapter {
            link,
            router,
            token,
            manga_id: 0,
            manga: MangaModel::default(),
            current_chapter_id: props.chapter_id,
            chapter: ChapterModel::default(),
            current_page: props.page.checked_sub(1).unwrap_or(0),
            chapters: vec![],
            previous_chapter_page: 0,
            pages: vec![],
            is_fetching: false,
            refs: vec![NodeRef::default(), NodeRef::default()],
            is_bar_visible: true,
            settings,
            page_refs: vec![],
            container_ref: NodeRef::default(),
            closure,
            is_history_fetching: false,
            worker: job::Worker::bridge(worker_callback),
            should_fetch: true,
        }
    }

    fn change(&mut self, props: Self::Properties) -> ShouldRender {
        if self.current_chapter_id != props.chapter_id
            || self.current_page != props.page.checked_sub(1).unwrap_or(0)
        {
            self.current_chapter_id = props.chapter_id;
            self.current_page = props.page.checked_sub(1).unwrap_or(0);
            return true;
        }
        false
    }

    fn rendered(&mut self, first_render: bool) {
        if first_render {
            if self.settings.page_rendering == PageRendering::DoublePage
                && self.current_page % 2 != 0
            {
                let route_string = format!(
                    "/chapter/{}/page/{}",
                    self.current_chapter_id, self.current_page
                );

                let route = Route::from(route_string);
                self.router.send(RouteRequest::ChangeRoute(route));
            }
        }
        if self.should_fetch {
            self.should_fetch = false;
            self.get_pages(false);
        }
        document()
            .get_element_by_id("manga-reader")
            .expect("should have manga reader")
            .dyn_ref::<HtmlElement>()
            .expect("should load HtmlElement")
            .focus()
            .unwrap();
    }

    fn update(&mut self, msg: Self::Message) -> ShouldRender {
        match msg {
            Msg::MangaReady(data) => {
                self.manga = data.manga.clone();
            }
            Msg::ChapterReady(data) => {
                self.is_fetching = false;
                self.chapters = data.chapters.clone();
                let idx = match self
                    .chapters
                    .iter()
                    .position(|chapter| chapter.id == self.current_chapter_id)
                {
                    Some(index) => index,
                    None => 0,
                };
                self.chapter = data.chapters[idx].clone();
                self.get_manga();
            }
            Msg::PagesReady(data) => {
                self.manga_id = data.manga_id;
                self.pages = data.pages;
                self.page_refs.clear();
                for _i in 0..self.pages.len() + 1 {
                    self.page_refs.push(NodeRef::default());
                }

                if self.settings.page_rendering == PageRendering::LongStrip {
                    match window().onscroll() {
                        Some(_) => {}
                        None => window().set_onscroll(Some(self.closure.as_ref().unchecked_ref())),
                    };
                }
                self.get_chapters();
                self.is_fetching = false;
            }
            Msg::PageForward => {
                if self.settings.page_rendering == PageRendering::LongStrip {
                    self.next_page_or_chapter();
                } else {
                    if self.settings.reading_direction == ReadingDirection::LeftToRight {
                        self.next_page_or_chapter();
                    } else {
                        self.prev_page_or_chapter();
                    }
                }
                self.set_history();
            }
            Msg::PagePrevious => {
                if self.settings.page_rendering == PageRendering::LongStrip {
                    self.prev_page_or_chapter();
                } else {
                    if self.settings.reading_direction == ReadingDirection::LeftToRight {
                        self.prev_page_or_chapter();
                    } else {
                        self.next_page_or_chapter();
                    }
                }
                self.set_history();
            }
            Msg::PageSliderChange(page) => {
                if self.settings.page_rendering == PageRendering::DoublePage && page % 2 != 0 {
                    self.move_to_page(page.checked_sub(1).unwrap_or(0))
                } else {
                    self.move_to_page(page);
                }
                self.set_history();
            }
            Msg::ToggleBar => {
                if self.is_bar_visible {
                    if let Some(bar) = self.refs[0].cast::<HtmlElement>() {
                        bar.class_list()
                            .remove_1("slideInDown")
                            .expect("failed remove class");
                        bar.class_list()
                            .add_1("slideOutUp")
                            .expect("failed add class");
                        self.is_bar_visible = false;
                    }
                    if let Some(bar) = self.refs[1].cast::<HtmlElement>() {
                        bar.class_list()
                            .remove_1("slideInUp")
                            .expect("failed remove class");
                        bar.class_list()
                            .add_1("slideOutDown")
                            .expect("failed add class");
                        self.is_bar_visible = false;
                    }
                } else {
                    if let Some(bar) = self.refs[0].cast::<HtmlElement>() {
                        bar.class_list()
                            .remove_1("slideOutUp")
                            .expect("failed remove class");
                        bar.class_list()
                            .add_1("slideInDown")
                            .expect("failed add class");
                        self.is_bar_visible = true;
                    }
                    if let Some(bar) = self.refs[1].cast::<HtmlElement>() {
                        bar.class_list()
                            .remove_1("slideOutDown")
                            .expect("failed remove class");
                        bar.class_list()
                            .add_1("slideInUp")
                            .expect("failed add class");
                        self.is_bar_visible = true;
                    }
                }
            }
            Msg::RouterCallback => {
                self.get_pages(false);
            }
            Msg::SetHistoryRequested => {
                self.is_history_fetching = false;
                return false;
            }
            Msg::ScrollEvent(scroll) => {
                let mut page = 0;
                for page_ref in self.page_refs.clone().iter() {
                    if let Some(el) = page_ref.cast::<HtmlImageElement>() {
                        if scroll > el.offset_top() as f64 {
                            page = el.id().parse::<usize>().unwrap();
                            if page == (self.pages.len().checked_sub(1).unwrap_or(0))
                                && page != self.current_page
                            {
                                self.current_page = page;
                                self.set_history();
                            }
                        } else {
                            if self.current_page != page {
                                self.current_page = page;
                                self.set_history();
                            }
                            break;
                        }
                    }
                }
            }
            Msg::Refresh => {
                self.get_pages(true);
            }
            Msg::Noop => {
                return false;
            }
        }
        true
    }

    fn view(&self) -> Html {
        let list_link = &WeakComponentLink::<PageList>::default();
        let on_mouse_up = if self.settings.page_rendering == PageRendering::LongStrip {
            self.link.callback(|_| Msg::ToggleBar)
        } else {
            self.link.callback(|_| Msg::Noop)
        };
        let onnextchapter = if self.settings.page_rendering == PageRendering::LongStrip {
            self.link.callback(|_| Msg::PageForward)
        } else {
            self.link.callback(|_| Msg::Noop)
        };
        let onprevchapter = if self.settings.page_rendering == PageRendering::LongStrip {
            self.link.callback(|_| Msg::PagePrevious)
        } else {
            self.link.callback(|_| Msg::Noop)
        };
        return html! {
        <div>
            <div
            ref=self.refs[0].clone()
            class="flex justify-between items-center animated slideInDown faster block fixed inset-x-0 top-0 z-50 bg-gray-900 z-50 content-end opacity-75"
            style="padding-top: calc(env(safe-area-inset-top) + .5rem)">
                <RouterAnchor<AppRoute> classes="z-50 mx-2 mb-2 text-white" route=AppRoute::Browse(BrowseRoute::Detail(self.manga_id))>
                    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" class="fill-current inline-block mb-1">
                        <path class="heroicon-ui" d="M5.41 11H21a1 1 0 0 1 0 2H5.41l5.3 5.3a1 1 0 0 1-1.42 1.4l-7-7a1 1 0 0 1 0-1.4l7-7a1 1 0 0 1 1.42 1.4L5.4 11z"/>
                    </svg>
               </RouterAnchor<AppRoute>>
               <div class="flex flex-col mx-2 mb-2">
                <span class="text-white text-center">{self.manga.title.to_owned()}</span>
                <span class="text-white text-center text-sm">{if let Some(v) = &self.chapter.vol {format!("Volume {}", v)} else if let Some(c) = &self.chapter.no {format!("Chapter {}", c)} else {"".to_string()}}</span>
               </div>
               <button
                onclick=self.link.callback(|_| Msg::Refresh)
                class="z-50 mx-2 mb-2 text-white ">
                    <svg class="fill-current inline-block mb-1 my-auto self-center" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" width="24" height="24" >
                        <path class="heroicon-ui" d="M6 18.7V21a1 1 0 0 1-2 0v-5a1 1 0 0 1 1-1h5a1 1 0 1 1 0 2H7.1A7 7 0 0 0 19 12a1 1 0 1 1 2 0 9 9 0 0 1-15 6.7zM18 5.3V3a1 1 0 0 1 2 0v5a1 1 0 0 1-1 1h-5a1 1 0 0 1 0-2h2.9A7 7 0 0 0 5 12a1 1 0 1 1-2 0 9 9 0 0 1 15-6.7z"/>
                    </svg>
                </button>
            </div>
            <div class="h-screen m-0 outline-none" id="manga-reader" tabindex="0" onkeydown=self.link.callback(|e: KeyboardEvent|
                match e.key().as_str() {
                    "ArrowRight" => Msg::PageForward,
                    "ArrowLeft"  => Msg::PagePrevious,
                    _ => Msg::Noop,
                }
            )>
                {
                    if self.settings.page_rendering != PageRendering::LongStrip {
                        html!{
                            <>
                                <button class="manga-navigate-left outline-none fixed" onmouseup=self.link.callback(|_| Msg::PagePrevious)/>
                                <button class="manga-navigate-center outline-none fixed" onmouseup=self.link.callback(|_| Msg::ToggleBar)/>
                                <button class="manga-navigate-right outline-none fixed" onmouseup=self.link.callback(|_| Msg::PageForward)/>
                            </>
                        }
                    } else {
                        html!{}
                    }
                }
                <PageList ref=self.container_ref.clone()
                    page_rendering={&self.settings.page_rendering}
                    reading_direction={&self.settings.reading_direction}
                    weak_link=list_link
                    current_page=self.current_page
                    onnextchapter=onnextchapter
                    onprevchapter=onprevchapter
                >
                    {
                        for self.pages
                            .clone()
                            .into_iter()
                            .enumerate()
                            .map(|(i, page)| {
                                html_nested! {
                                    <Page
                                        id={i}
                                        key={i}
                                        page_ref=self.page_refs[i].clone()
                                        hidden={self.current_page != i}
                                        page_rendering={&self.settings.page_rendering}
                                        reading_direction={&self.settings.reading_direction}
                                        onmouseup={&on_mouse_up}
                                        src={if i >= 0 && i < self.current_page + 2 {page} else {"".to_string()}}
                                    />
                                }
                            })
                    }
                </PageList>
                <Spinner is_active=self.is_fetching is_fullscreen=true/>
            </div>
            <div ref=self.refs[1].clone()
            class="animated slideInUp faster block fixed inset-x-0 bottom-0 z-50 bg-gray-900 opacity-75 shadow safe-bottom">
                <div class="flex px-4 py-5 justify-center">
                    <span class="mx-4 text-white">{format!("{}", self.current_page + 1)}</span>
                    <input
                        type="range"
                        min="0"
                        max=self.pages.len().checked_sub(1).unwrap_or(0)
                        step="1"
                        value={self.current_page}
                        oninput=self.link.callback(|e: InputData| Msg::PageSliderChange(e.value.parse::<usize>().unwrap()))/>
                    <span class="mx-4 text-white">{format!("{}", self.pages.len())}</span>
                </div>
            </div>
        </div>
        };
    }

    fn destroy(&mut self) {
        if self.settings.page_rendering == PageRendering::LongStrip {
            window().set_onscroll(None);
        }
        if self.settings.background_color == BackgroundColor::Black {
            document()
                .body()
                .expect("document should have a body")
                .dyn_ref::<web_sys::HtmlElement>()
                .unwrap()
                .style()
                .set_property("background-color", "white")
                .expect("failed to set background color");
        }
    }
}

impl Chapter {
    fn get_manga(&mut self) {
        self.worker.send(job::Request::FetchManga(self.manga_id));
    }

    fn get_chapters(&mut self) {
        self.worker
            .send(job::Request::FetchChapters(self.manga_id, false));
    }

    fn get_pages(&mut self, refresh: bool) {
        self.worker
            .send(job::Request::FetchPages(self.current_chapter_id, refresh));
        self.is_fetching = true;
    }

    fn move_to_page(&mut self, page: usize) {
        self.current_page = page;
        if self.settings.page_rendering == PageRendering::LongStrip {
            if let Some(el) = self.page_refs[page].cast::<HtmlImageElement>() {
                el.scroll_into_view();
            }
        } else {
        }
    }

    fn next_page_or_chapter(&mut self) {
        let mut num = 1;
        if self.settings.page_rendering == PageRendering::DoublePage {
            num = 2;
        }

        let mut current_page = self.current_page + num;
        current_page = match self.pages.get(current_page) {
            Some(_) => current_page,
            None => 0,
        };

        let route_string: String;
        if current_page == 0 {
            let current_chapter_idx = match self
                .chapters
                .iter()
                .position(|chapter| chapter.id == self.current_chapter_id)
            {
                Some(index) => index,
                None => 0,
            };

            let is_next = match current_chapter_idx.checked_sub(1) {
                Some(index) => {
                    self.current_chapter_id = self.chapters[index].id;
                    true
                }
                None => false,
            };

            if is_next {
                self.pages.clear();
                route_string = format!("/chapter/{}/page/1", self.current_chapter_id);
                self.current_page = current_page;
                self.previous_chapter_page = self.current_page;

                let route = Route::from(route_string);
                self.router.send(RouteRequest::ChangeRoute(route));
            }
        } else {
            self.current_page = current_page;
            route_string = format!(
                "/chapter/{}/page/{}",
                self.current_chapter_id,
                self.current_page + 1
            );
            let route = Route::from(route_string);
            self.router
                .send(RouteRequest::ReplaceRouteNoBroadcast(route));
        }
    }

    fn prev_page_or_chapter(&mut self) {
        let mut num: usize = 1;
        if self.settings.page_rendering == PageRendering::DoublePage {
            num = 2;
        }

        let is_prev = match self.current_page.checked_sub(num) {
            Some(page) => {
                self.current_page = page;
                false
            }
            None => true,
        };

        if is_prev {
            let current_chapter_idx = match self
                .chapters
                .iter()
                .position(|chapter| chapter.id == self.current_chapter_id)
            {
                Some(index) => index + 1,
                None => 0,
            };

            self.current_chapter_id = match self.chapters.get(current_chapter_idx) {
                Some(chapter) => chapter.id,
                None => self.current_chapter_id,
            };
            self.current_page = self.previous_chapter_page;
            if current_chapter_idx < self.chapters.len() {
                self.pages.clear();
                let route_string = format!(
                    "/chapter/{}/page/{}",
                    self.current_chapter_id,
                    self.current_page + 1
                );
                let route = Route::from(route_string);
                self.router.send(RouteRequest::ChangeRoute(route));
            }
        } else {
            let route_string = format!(
                "/chapter/{}/page/{}",
                self.current_chapter_id,
                self.current_page + 1
            );
            let route = Route::from(route_string);
            self.router
                .send(RouteRequest::ReplaceRouteNoBroadcast(route));
        }
    }

    fn get_date(&self) -> chrono::NaiveDateTime {
        let timestamp = js_sys::Date::now();
        let secs: i64 = (timestamp / 1000.0).floor() as i64;
        let nanoes: u32 = (timestamp as u32 % 1000) * 1_000_000;
        chrono::NaiveDateTime::from_timestamp(secs, nanoes)
    }

    fn set_history(&mut self) {
        let h = HistoryRequest {
            chapter_id: self.current_chapter_id,
            read: self.current_page as i32,
            at: self.get_date(),
        };
        self.worker
            .send(job::Request::PostHistory(self.token.clone(), h));
    }
}
