//! RayX's theme service, NSH-free: the state and its logic. What NSH shows reaches it from the
//! surface's bridge ([`RayxTheme::observe`]); what it decides leaves as [`Action`]s, which the
//! bridge applies on the engine thread. Outside an engine there is no bridge: the commands still
//! validate and set the choice, and nothing is picked.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use futures::FutureExt as _;
use futures::channel::mpsc::UnboundedSender;
use futures::future::{BoxFuture, LocalBoxFuture};
use rayx_app::ChangeSubscription;
use rayx_app::StateCell;
use rayx_app::di::{
    AsyncInitializable, StartupInitializable, StartupInitializationContext,
    StartupInitializationResult, StartupServiceError, StartupServiceType,
};
use rayx_app::services::platform::{ColorScheme, PlatformInfoService};
use rayx_app::services::theme::{
    ColorModeChoice, ThemeError, ThemeModifier, ThemeService, ThemeSnapshot,
};

use crate::options::ThemeOptions;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the service asks of NSH.
#[derive(Debug)]
pub(crate) enum Action {
    /// `set_modifier(modifier, context)`.
    Pick { modifier: String, context: String },
    /// `set_color_scheme` with the appearance of the choice (`System`: the system's).
    Appearance(ColorModeChoice),
    /// Answered once the actions sent before it were applied.
    Flushed(futures::channel::oneshot::Sender<()>),
}

/// What the builder's provisional pick did, for the service to carry on from.
#[derive(Clone, Debug, Default)]
pub(crate) struct Provisional {
    /// The light/dark context it picked, until NSH shows it.
    pub(crate) color_pick: Option<String>,
    /// The appearance it set NSH to.
    pub(crate) appearance: Option<ColorModeChoice>,
}

/// The bridge's end the service sends actions to.
pub(crate) struct Bridge {
    pub(crate) actions: UnboundedSender<Action>,
}

struct State {
    /// The options in force: `configure`'s until startup, then the bound ones.
    options: ThemeOptions,
    color_mode: ColorModeChoice,
    /// The color scheme the platform service reports.
    scheme: ColorScheme,
    /// The active document's modifiers as the bridge last saw them.
    modifiers: Vec<ThemeModifier>,
    /// Whether the bridge saw NSH's modifiers yet.
    observed: bool,
    /// The light/dark contexts the service picked that NSH does not show yet, oldest first.
    color_picks: VecDeque<String>,
    /// The appearance the service last set NSH to (`None`: never).
    appearance: Option<ColorModeChoice>,
    bridge: Option<Bridge>,
}

impl State {
    /// The light/dark modifier as NSH shows it.
    fn color_axis(&self) -> Option<&ThemeModifier> {
        let name = &self.options.color_mode.modifier;
        self.modifiers
            .iter()
            .find(|modifier| modifier.name == *name)
    }

    /// Sends `action` to the bridge; whether there is one.
    fn send(&self, action: Action) -> bool {
        self.bridge
            .as_ref()
            .is_some_and(|bridge| bridge.actions.unbounded_send(action).is_ok())
    }

    /// Asks NSH for the appearance and the light/dark context the choice and the scheme call
    /// for, where they differ from what it shows or was asked.
    fn apply_color_mode(&mut self) {
        let choice = self.color_mode;
        let appearance = match choice {
            ColorModeChoice::Light | ColorModeChoice::Dark => Some(choice),
            // Back to the system's only when the service set one of its own.
            ColorModeChoice::System => matches!(
                self.appearance,
                Some(ColorModeChoice::Light | ColorModeChoice::Dark)
            )
            .then_some(ColorModeChoice::System),
        };
        if let Some(appearance) = appearance
            && self.appearance != Some(appearance)
            && self.send(Action::Appearance(appearance))
        {
            self.appearance = Some(appearance);
        }
        let options = &self.options.color_mode;
        let wanted = options.context_for(choice, self.scheme).to_owned();
        let Some(axis) = self.color_axis() else {
            // A document without the modifier: the choice waits for one that has it.
            return;
        };
        if !axis.has_context(&wanted) {
            return;
        }
        let shown = self.color_picks.back().unwrap_or(&axis.selected);
        if *shown == wanted {
            return;
        }
        let modifier = options.modifier.clone();
        if self.send(Action::Pick {
            modifier,
            context: wanted.clone(),
        }) {
            self.color_picks.push_back(wanted);
        }
    }

    /// The snapshot of the state, at `revision`.
    fn snapshot(&self, revision: u64) -> ThemeSnapshot {
        ThemeSnapshot {
            modifiers: self.modifiers.clone(),
            color_mode: self.color_mode,
            revision,
        }
    }
}

/// RayX's theme service on one surface.
pub(crate) struct RayxTheme {
    theme: StateCell<ThemeSnapshot>,
    state: Mutex<State>,
    /// Held while a snapshot is published; a change meanwhile marks it stale.
    publishing: Mutex<()>,
    stale: AtomicBool,
    initialized: AtomicBool,
    platform: Mutex<Option<ChangeSubscription>>,
    /// Itself, for the bridge its startup starts.
    me: Weak<Self>,
}

impl RayxTheme {
    /// A service at `options` (`configure`'s) after the builder's provisional pick.
    pub(crate) fn new(options: ThemeOptions, provisional: Provisional) -> Arc<Self> {
        let color_mode = options.color_mode.choice;
        Arc::new_cyclic(|me| Self {
            theme: StateCell::new(ThemeSnapshot {
                color_mode,
                ..ThemeSnapshot::default()
            }),
            state: Mutex::new(State {
                options,
                color_mode,
                scheme: ColorScheme::Light,
                modifiers: Vec::new(),
                observed: false,
                color_picks: provisional.color_pick.into_iter().collect(),
                appearance: provisional.appearance,
                bridge: None,
            }),
            publishing: Mutex::new(()),
            stale: AtomicBool::new(false),
            initialized: AtomicBool::new(false),
            platform: Mutex::new(None),
            me: me.clone(),
        })
    }

    /// Connects the bridge: from now on actions go to it.
    pub(crate) fn connect(&self, bridge: Bridge) {
        lock(&self.state).bridge = Some(bridge);
    }

    /// What the bridge saw NSH show: the active document's modifiers. The first time, the
    /// service applies its start picks and color mode; after that, a light/dark context picked
    /// around the service is adopted as the choice, and a document that brings the light/dark
    /// modifier back gets the choice's context.
    pub(crate) fn observe(&self, modifiers: Vec<ThemeModifier>) {
        {
            let mut state = lock(&self.state);
            let before = state.color_axis().map(|axis| axis.selected.clone());
            state.modifiers = modifiers;
            let now = state.color_axis().map(|axis| axis.selected.clone());
            if !state.observed {
                state.observed = true;
                if let Some(now) = &now
                    && let Some(at) = state.color_picks.iter().position(|pick| pick == now)
                {
                    state.color_picks.drain(..=at);
                }
                self.start_picks(&mut state);
            } else {
                match (before, now) {
                    (Some(before), Some(now)) if before != now => {
                        if let Some(at) = state.color_picks.iter().position(|pick| *pick == now) {
                            // The service's own pick, now shown.
                            state.color_picks.drain(..=at);
                        } else if let Some(choice) = state.options.color_mode.choice_of(&now) {
                            // Picked around the service: the app chose it.
                            state.color_picks.clear();
                            state.color_mode = choice;
                            state.apply_color_mode();
                        } else {
                            // Another context of the modifier: shown, the choice kept.
                            state.color_picks.clear();
                        }
                    }
                    (None, Some(_)) => {
                        // A document with the modifier became active: it shows the choice.
                        state.color_picks.clear();
                        state.apply_color_mode();
                    }
                    (Some(_), None) => state.color_picks.clear(),
                    _ => {}
                }
            }
        }
        self.publish();
    }

    /// Applies the start picks and the color mode the options call for.
    fn start_picks(&self, state: &mut State) {
        let picks: Vec<(String, String)> = state
            .options
            .valid_contexts()
            .map(|(modifier, context)| (modifier.to_owned(), context.to_owned()))
            .collect();
        for (modifier, context) in picks {
            match state.modifiers.iter().find(|axis| axis.name == modifier) {
                None => eprintln!(
                    "rayx: the theme has no modifier `{modifier}`: its start context `{context}` \
                     is skipped"
                ),
                Some(axis) if !axis.has_context(&context) => eprintln!(
                    "rayx: the theme's modifier `{modifier}` has no context `{context}`: it is \
                     skipped"
                ),
                Some(axis) if axis.selected == context => {}
                Some(_) => {
                    state.send(Action::Pick { modifier, context });
                }
            }
        }
        state.apply_color_mode();
    }

    /// Starts the service with the bound `options`, following `platform`'s color scheme.
    fn start(self: &Arc<Self>, options: ThemeOptions, platform: &Arc<dyn PlatformInfoService>) {
        {
            let mut state = lock(&self.state);
            state.color_mode = options.color_mode.choice;
            state.options = options;
            state.scheme = platform.platform().get().color_scheme;
        }
        let weak: Weak<Self> = Arc::downgrade(self);
        let subscription = platform.platform().subscribe(move |snapshot| {
            if let Some(theme) = weak.upgrade() {
                theme.follow_scheme(snapshot.color_scheme);
            }
        });
        *lock(&self.platform) = Some(subscription);
        self.publish();
    }

    /// The platform reported `scheme`: under `System`, the light/dark modifier follows it.
    fn follow_scheme(&self, scheme: ColorScheme) {
        let mut state = lock(&self.state);
        if state.scheme == scheme {
            return;
        }
        state.scheme = scheme;
        if state.observed && state.color_mode == ColorModeChoice::System {
            state.apply_color_mode();
        }
    }

    /// Publishes the state unless it is the snapshot published (whatever its revision), at the
    /// next revision: a change made meanwhile is published right after, by whoever publishes.
    fn publish(&self) {
        self.stale.store(true, Ordering::SeqCst);
        loop {
            let publishing = match self.publishing.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
                Err(std::sync::TryLockError::WouldBlock) => return,
            };
            self.stale.store(false, Ordering::SeqCst);
            let published = self.theme.get();
            let snapshot = lock(&self.state).snapshot(published.revision + 1);
            if snapshot.modifiers != published.modifiers
                || snapshot.color_mode != published.color_mode
            {
                self.theme.publish(snapshot);
            }
            drop(publishing);
            if !self.stale.load(Ordering::SeqCst) {
                return;
            }
        }
    }

    fn pick(&self, modifier: String, context: String) -> Result<bool, ThemeError> {
        let changed = {
            let mut state = lock(&self.state);
            let options = &state.options.color_mode;
            let color = modifier == options.modifier;
            if color && context != options.light && context != options.dark {
                return Err(ThemeError::UnknownContext { modifier, context });
            }
            let snapshot = self.theme.get();
            let Some(axis) = snapshot.modifier(&modifier) else {
                return Err(ThemeError::UnknownModifier { modifier });
            };
            if !axis.has_context(&context) {
                return Err(ThemeError::UnknownContext { modifier, context });
            }
            let changed = axis.selected != context;
            if color {
                state.color_mode = state
                    .options
                    .color_mode
                    .choice_of(&context)
                    .unwrap_or(state.color_mode);
                state.apply_color_mode();
            } else {
                state.send(Action::Pick { modifier, context });
            }
            changed
        };
        self.publish();
        Ok(changed)
    }

    fn choose(&self, choice: ColorModeChoice) -> bool {
        {
            let mut state = lock(&self.state);
            if state.color_mode == choice {
                return false;
            }
            state.color_mode = choice;
            state.apply_color_mode();
        }
        self.publish();
        true
    }
}

impl ThemeService for RayxTheme {
    fn theme(&self) -> &StateCell<ThemeSnapshot> {
        &self.theme
    }

    fn set_context(
        &self,
        modifier: String,
        context: String,
    ) -> BoxFuture<'_, Result<bool, ThemeError>> {
        futures::future::ready(self.pick(modifier, context)).boxed()
    }

    fn set_color_mode(&self, choice: ColorModeChoice) -> BoxFuture<'_, Result<bool, ThemeError>> {
        futures::future::ready(Ok(self.choose(choice))).boxed()
    }
}

/// The service starts with the app: it reads its bound options and the platform, then starts
/// its bridge where an engine runs (on NSH, the engine thread, where startup runs).
impl AsyncInitializable for RayxTheme {
    fn is_initialized(&self) -> bool {
        self.initialized.load(Ordering::SeqCst)
    }

    fn initialize(
        &self,
        context: StartupInitializationContext,
    ) -> LocalBoxFuture<'static, StartupInitializationResult> {
        let theme = self.me.upgrade();
        let services = context.services().clone();
        async move {
            let theme =
                theme.ok_or_else(|| StartupServiceError::new("the theme service is gone"))?;
            let options = services
                .options::<ThemeOptions>()
                .map_err(|error| {
                    StartupServiceError::new(format!("the theme options are invalid: {error}"))
                })?
                .cloned();
            options.validate().map_err(StartupServiceError::new)?;
            let platform = services
                .required_service::<dyn PlatformInfoService>()
                .map_err(|error| {
                    StartupServiceError::new(format!(
                        "the theme service needs the platform: {error}"
                    ))
                })?;
            theme.start(options, &platform);
            crate::nsh::start_bridge(&theme).await;
            theme.initialized.store(true, Ordering::SeqCst);
            Ok(())
        }
        .boxed_local()
    }
}

impl StartupInitializable for RayxTheme {
    fn startup_service_type(&self) -> Option<StartupServiceType> {
        Some(StartupServiceType::of::<dyn ThemeService>())
    }

    fn initialization_depends_on(&self) -> Vec<StartupServiceType> {
        vec![StartupServiceType::of::<dyn PlatformInfoService>()]
    }
}
