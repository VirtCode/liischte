// more or less copied and modified from https://github.com/pop-os/libcosmic/blob/master/src/app/multi_window.rs

use iced::application;
use iced::window;
use iced::{
    self, Program,
    program::{self, with_style, with_subscription, with_theme, with_title},
    runtime::{Appearance, DefaultStyle},
};
use iced::{Element, Result, Settings, Subscription, Task};

use std::marker::PhantomData;

pub(crate) struct Instance<State, Message, Theme, Renderer, Update, View, Executor> {
    update: Update,
    view: View,
    _state: PhantomData<State>,
    _message: PhantomData<Message>,
    _theme: PhantomData<Theme>,
    _renderer: PhantomData<Renderer>,
    _executor: PhantomData<Executor>,
}

impl<State, Message, Theme, Renderer, Update, View, Executor> Program
    for Instance<State, Message, Theme, Renderer, Update, View, Executor>
where
    Message: Send + std::fmt::Debug + 'static,
    Theme: Default + DefaultStyle,
    Renderer: program::Renderer,
    Update: application::Update<State, Message>,
    View: for<'a> self::View<'a, State, Message, Theme, Renderer>,
    Executor: iced::Executor,
{
    type State = State;
    type Message = Message;
    type Theme = Theme;
    type Renderer = Renderer;
    type Executor = Executor;

    fn update(&self, state: &mut Self::State, message: Self::Message) -> Task<Self::Message> {
        self.update.update(state, message).into()
    }

    fn view<'a>(
        &self,
        state: &'a Self::State,
        window: window::Id,
    ) -> Element<'a, Self::Message, Self::Theme, Self::Renderer> {
        self.view.view(state, window).into()
    }
}

/// Creates an iced [`MultiWindow`] given its title, update, and view logic.
pub fn layer_window<State, Message, Theme, Renderer, Executor>(
    title: impl Title<State>,
    update: impl application::Update<State, Message>,
    view: impl for<'a> self::View<'a, State, Message, Theme, Renderer>,
) -> LayerMultiWindow<impl Program<State = State, Message = Message, Theme = Theme>>
where
    State: 'static,
    Message: Send + std::fmt::Debug + 'static,
    Theme: Default + DefaultStyle,
    Renderer: program::Renderer,
    Executor: iced::Executor,
{
    use std::marker::PhantomData;

    LayerMultiWindow {
        raw: Instance {
            update,
            view,
            _state: PhantomData,
            _message: PhantomData,
            _theme: PhantomData,
            _renderer: PhantomData,
            _executor: PhantomData::<Executor>,
        },
        settings: Settings::default(),
        window: None,
    }
    .title(title)
}

/// An iced daemon window which allows to run without a main window
///
/// It can be used for applications with layershell surfaces
#[derive(Debug)]
pub struct LayerMultiWindow<P: Program> {
    raw: P,
    settings: Settings,
    window: Option<window::Settings>,
}

impl<P: Program> LayerMultiWindow<P> {
    /// Runs the [`LayerMultiWindow`]
    ///
    /// Should the application not implement [`Default`] one should use
    /// [`run_with`] instead
    ///
    /// [`run_with`]: Self::run_with
    pub fn run(self) -> Result
    where
        Self: 'static,
        P::State: Default,
    {
        self.raw.run(self.settings, self.window)
    }

    /// Runs the [`LayerMultiWindow`] with a closure that creates the initial
    /// state
    pub fn run_with<I>(self, initialize: I) -> Result
    where
        Self: 'static,
        I: FnOnce() -> (P::State, Task<P::Message>) + 'static,
    {
        self.raw.run_with(self.settings, self.window, initialize)
    }

    /// Sets the [`Settings`] that will be used to run the [`LayerMultiWindow`]
    pub fn settings(self, settings: Settings) -> Self {
        Self { settings, ..self }
    }

    /// Sets the [`Title`] of the [`LayerMultiWindow`].
    pub(crate) fn title(
        self,
        title: impl Title<P::State>,
    ) -> LayerMultiWindow<impl Program<State = P::State, Message = P::Message, Theme = P::Theme>>
    {
        LayerMultiWindow {
            raw: with_title(self.raw, move |state, window| title.title(state, window)),
            settings: self.settings,
            window: self.window,
        }
    }

    /// Sets the subscription logic of the [`LayerMultiWindow`]
    pub fn subscription(
        self,
        f: impl Fn(&P::State) -> Subscription<P::Message>,
    ) -> LayerMultiWindow<impl Program<State = P::State, Message = P::Message, Theme = P::Theme>>
    {
        LayerMultiWindow {
            raw: with_subscription(self.raw, f),
            settings: self.settings,
            window: self.window,
        }
    }

    /// Sets the theme logic of the [`LayerMultiWindow`]
    pub fn theme(
        self,
        f: impl Fn(&P::State, window::Id) -> P::Theme,
    ) -> LayerMultiWindow<impl Program<State = P::State, Message = P::Message, Theme = P::Theme>>
    {
        LayerMultiWindow {
            raw: with_theme(self.raw, f),
            settings: self.settings,
            window: self.window,
        }
    }

    /// Sets the style logic of the [`LayerMultiWindow`]
    pub fn style(
        self,
        f: impl Fn(&P::State, &P::Theme) -> Appearance,
    ) -> LayerMultiWindow<impl Program<State = P::State, Message = P::Message, Theme = P::Theme>>
    {
        LayerMultiWindow {
            raw: with_style(self.raw, f),
            settings: self.settings,
            window: self.window,
        }
    }

    /// Sets the window settings of the [`LayerMultiWindow`]
    ///
    /// These settings are for the main window and not for the layershell
    /// surfaces
    ///
    /// When `None` then no main window is displayed
    pub fn window(self, window: Option<window::Settings>) -> Self {
        Self { raw: self.raw, settings: self.settings, window }
    }
}

/// The title logic of some [`LayerMultiWindow`]
pub trait Title<State> {
    /// Produces the title of the [`LayerMultiWindow`].
    fn title(&self, state: &State, window: window::Id) -> String;
}

impl<State> Title<State> for &'static str {
    fn title(&self, _state: &State, _window: window::Id) -> String {
        (*self).to_string()
    }
}

impl<T, State> Title<State> for T
where
    T: Fn(&State, window::Id) -> String,
{
    fn title(&self, state: &State, window: window::Id) -> String {
        self(state, window)
    }
}

/// The view logic of some [`LayerMultiWindow`]
pub trait View<'a, State, Message, Theme, Renderer> {
    /// Produces the widget of the [`LayerMultiWindow`].
    fn view(
        &self,
        state: &'a State,
        window: window::Id,
    ) -> impl Into<Element<'a, Message, Theme, Renderer>>;
}

impl<'a, T, State, Message, Theme, Renderer, Widget> View<'a, State, Message, Theme, Renderer> for T
where
    T: Fn(&'a State, window::Id) -> Widget,
    State: 'static,
    Widget: Into<Element<'a, Message, Theme, Renderer>>,
{
    fn view(
        &self,
        state: &'a State,
        window: window::Id,
    ) -> impl Into<Element<'a, Message, Theme, Renderer>> {
        self(state, window)
    }
}
