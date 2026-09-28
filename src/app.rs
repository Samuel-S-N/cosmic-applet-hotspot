// SPDX-License-Identifier: MPL-2.0

use crate::config::{Band, Settings, Stored};
use crate::idle::IdleTracker;
use crate::nm::{self, HotspotState};
use crate::ui::{self, ToggleAction};
use cosmic::cosmic_config::{self, CosmicConfigEntry};
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::iced::{window::Id, Limits, Subscription};
use cosmic::prelude::*;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Main,
    Settings,
}

#[derive(Default)]
pub struct AppModel {
    pub core: cosmic::Core,
    pub popup: Option<Id>,
    pub tab: Tab,
    pub snap: Option<nm::Snapshot>,
    /// Draft edited in the settings tab.
    pub settings: Settings,
    /// Confirm warning (continue/cancel) visible.
    pub confirm: bool,
    pub error: Option<String>,
    pub idle: IdleTracker,
    pub config_handler: Option<cosmic_config::Config>,
    /// Last saved idle timeout; the unsaved draft never drives auto-shutdown.
    pub saved_idle_minutes: u32,
    /// Activation dispatched; ticks seen without reaching On (u32::MAX = saw Starting).
    pub pending_on: Option<u32>,
    /// A start/stop/apply request is in flight; the toggle ignores further clicks.
    pub busy: bool,
    /// The password draft was edited by the user; NM's stored password must not replace it.
    pub password_dirty: bool,
    /// Password shown as plain text instead of dots in the settings tab.
    pub password_visible: bool,
}

/// Advances the pending-activation tracker; `true` when activation failed silently.
fn start_failed(pending: &mut Option<u32>, state: HotspotState) -> bool {
    let Some(n) = *pending else { return false };
    match state {
        HotspotState::On => *pending = None,
        HotspotState::Starting => *pending = Some(u32::MAX),
        HotspotState::Off if n == u32::MAX || n + 1 >= 3 => {
            *pending = None;
            return true;
        }
        HotspotState::Off => *pending = Some(n + 1),
    }
    false
}

#[derive(Debug, Clone)]
pub enum Message {
    LanguageChanged(Vec<String>),
    TogglePopup,
    PopupClosed(Id),
    Tab(Tab),
    Tick,
    Snap(Result<nm::Snapshot, String>),
    Toggle(bool),
    ConfirmYes,
    ConfirmNo,
    Done(Result<(), String>),
    SsidChanged(String),
    PasswordChanged(String),
    TogglePasswordVisible,
    BandChanged(usize),
    IdleChanged(String),
    Save,
    PasswordLoaded(Result<String, String>),
    UpdateConfig(Stored),
}

type Cmd = Task<cosmic::Action<Message>>;

fn app_task<T: Send + 'static>(
    fut: impl std::future::Future<Output = T> + Send + 'static,
    f: impl Fn(T) -> Message + Send + 'static,
) -> Cmd {
    Task::perform(fut, move |r| cosmic::Action::App(f(r)))
}

fn tick() -> Cmd {
    app_task(nm::snapshot(), Message::Snap)
}

impl AppModel {
    fn run(&mut self, fut: impl std::future::Future<Output = Result<(), String>> + Send + 'static) -> Cmd {
        self.busy = true;
        app_task(fut, Message::Done)
    }

    fn apply_stored(&mut self, s: Stored) {
        self.settings.ssid = s.ssid;
        self.settings.band = s.band;
        self.settings.idle_minutes = s.idle_minutes;
        self.saved_idle_minutes = s.idle_minutes;
    }
}

impl cosmic::Application for AppModel {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "com.samuel.CosmicAppletHotspot";

    fn core(&self) -> &cosmic::Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }

    fn init(core: cosmic::Core, _flags: Self::Flags) -> (Self, Cmd) {
        let mut app = AppModel { core, ..Default::default() };
        app.saved_idle_minutes = app.settings.idle_minutes; // default (10) survives an unreadable config
        if let Ok(ctx) = cosmic_config::Config::new(Self::APP_ID, Stored::VERSION) {
            let stored = match Stored::get_entry(&ctx) {
                Ok(s) => s,
                Err((_errors, s)) => s,
            };
            app.apply_stored(stored);
            app.config_handler = Some(ctx);
        }
        (
            app,
            Task::batch([tick(), app_task(crate::language::current(), Message::LanguageChanged)]),
        )
    }

    fn on_close_requested(&self, id: Id) -> Option<Message> {
        Some(Message::PopupClosed(id))
    }

    fn view(&self) -> Element<'_, Self::Message> {
        ui::panel_button(self)
    }

    fn view_window(&self, _id: Id) -> Element<'_, Self::Message> {
        ui::view(self)
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        Subscription::batch(vec![
            cosmic::iced::time::every(Duration::from_secs(5)).map(|_| Message::Tick),
            self.core()
                .watch_config::<Stored>(Self::APP_ID)
                .map(|update| Message::UpdateConfig(update.config)),
            Subscription::run(crate::language::changes).map(Message::LanguageChanged),
        ])
    }

    fn update(&mut self, message: Self::Message) -> Cmd {
        match message {
            Message::LanguageChanged(raw) => {
                let langs = crate::i18n::parse_languages(&raw);
                if !langs.is_empty() {
                    crate::i18n::select(&langs);
                }
                // Receiving any message makes iced rebuild `view()`, which re-evaluates `fl!`.
            }
            Message::Toggle(_) if self.busy => {}
            Message::UpdateConfig(s) => self.apply_stored(s),
            Message::TogglePopup => {
                return if let Some(p) = self.popup.take() {
                    destroy_popup(p)
                } else {
                    self.password_visible = false;
                    let new_id = Id::unique();
                    self.popup.replace(new_id);
                    let mut popup_settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        new_id,
                        None,
                        None,
                        None,
                    );
                    popup_settings.positioner.size_limits = Limits::NONE
                        .max_width(372.0)
                        .min_width(300.0)
                        .min_height(200.0)
                        .max_height(1080.0);
                    Task::batch([
                        get_popup(popup_settings),
                        tick(),
                        app_task(nm::load_password(), Message::PasswordLoaded),
                    ])
                };
            }
            Message::PopupClosed(id) => {
                if self.popup.as_ref() == Some(&id) {
                    self.popup = None;
                    self.confirm = false;
                }
            }
            Message::Tab(t) => self.tab = t,
            Message::Tick => return tick(),
            Message::Snap(Ok(s)) => {
                let on = s.state == HotspotState::On;
                if start_failed(&mut self.pending_on, s.state) {
                    self.error = Some(crate::fl!("err-activate"));
                }
                let stop = self.idle.tick(on, s.clients.len(), Instant::now(), self.saved_idle_minutes);
                self.snap = Some(s);
                if stop {
                    self.pending_on = None;
                    return self.run(nm::down());
                }
            }
            Message::Snap(Err(e)) => self.error = Some(e),
            Message::Toggle(want) => {
                if let Some(snap) = &self.snap {
                    match ui::toggle_action(want, self.settings.validate().is_ok(), snap) {
                        ToggleAction::TurnOff => {
                            self.pending_on = None;
                            return self.run(nm::down());
                        }
                        ToggleAction::TurnOn => {
                            self.pending_on = Some(0);
                            let s = self.settings.clone();
                            return self.run(async move { nm::up(&s).await });
                        }
                        ToggleAction::AskConfirm => self.confirm = true,
                        ToggleAction::NeedsSetup => {
                            self.tab = Tab::Settings;
                            self.error = Some(crate::fl!("need-setup"));
                        }
                    }
                }
            }
            Message::ConfirmYes => {
                self.confirm = false;
                self.pending_on = Some(0);
                let s = self.settings.clone();
                return self.run(async move { nm::up(&s).await });
            }
            Message::ConfirmNo => self.confirm = false,
            Message::TogglePasswordVisible => self.password_visible = !self.password_visible,
            Message::Done(Err(e)) => {
                self.busy = false;
                self.pending_on = None;
                self.error = Some(e);
            }
            Message::Done(Ok(())) => {
                self.busy = false;
                self.error = None;
                return tick();
            }
            Message::SsidChanged(v) => self.settings.ssid = v,
            Message::PasswordChanged(v) => {
                self.settings.password = v;
                self.password_dirty = true;
            }
            Message::BandChanged(i) => {
                self.settings.band = if i == 1 { Band::Ghz5 } else { Band::Ghz24 };
            }
            Message::IdleChanged(v) => {
                if v.chars().all(|c| c.is_ascii_digit()) {
                    self.settings.idle_minutes = v.parse().unwrap_or(0);
                }
            }
            Message::Save => match self.settings.validate() {
                Err(e) => self.error = Some(e),
                Ok(()) => {
                    if let Some(h) = &self.config_handler {
                        let stored = Stored {
                            ssid: self.settings.ssid.clone(),
                            band: self.settings.band,
                            idle_minutes: self.settings.idle_minutes,
                        };
                        if let Err(e) = stored.write_entry(h) {
                            self.error = Some(crate::fl!("err-save-config", error = e.to_string()));
                            return Task::none();
                        }
                    }
                    self.saved_idle_minutes = self.settings.idle_minutes;
                    self.password_dirty = false;
                    if matches!(&self.snap, Some(sn) if sn.state != HotspotState::Off) {
                        self.pending_on = Some(0);
                    }
                    let s = self.settings.clone();
                    return self.run(async move { nm::apply_settings(&s).await });
                }
            },
            Message::PasswordLoaded(Ok(p)) => {
                if !self.password_dirty {
                    self.settings.password = p;
                }
            }
            Message::PasswordLoaded(Err(_)) => {}
        }
        Task::none()
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cosmic::Application as _;

    fn unconfigured_with_snapshot(uplink_ok: bool) -> AppModel {
        let mut m = AppModel::default();
        m.snap = Some(nm::Snapshot {
            state: HotspotState::Off,
            clients: vec![],
            uplink_ok,
            wifi_client_connected: false,
        });
        m
    }

    /// Settings the user already filled in (there are no built-in credentials).
    fn model_with_snapshot(uplink_ok: bool) -> AppModel {
        let mut m = unconfigured_with_snapshot(uplink_ok);
        m.settings = Settings { ssid: "MyNetwork".into(), password: "test-password".into(), ..Settings::default() };
        m
    }

    #[test]
    fn turning_on_without_setup_opens_settings() {
        let mut m = unconfigured_with_snapshot(true);
        let _ = m.update(Message::Toggle(true));
        assert_eq!(m.tab, Tab::Settings);
        assert!(m.error.is_some(), "tell the user what to fill in");
        assert!(!m.busy, "nothing was dispatched");
        assert_eq!(m.pending_on, None);
    }

    #[test]
    fn second_toggle_while_busy_is_ignored() {
        let mut m = model_with_snapshot(true);
        let _ = m.update(Message::Toggle(true));
        assert!(m.busy);
        assert_eq!(m.pending_on, Some(0));
        m.pending_on = None; // a second dispatch would set it again
        let _ = m.update(Message::Toggle(true));
        assert_eq!(m.pending_on, None, "toggle while busy must not dispatch");
        let _ = m.update(Message::Done(Ok(())));
        assert!(!m.busy);
    }

    #[test]
    fn loaded_password_does_not_overwrite_unsaved_edit() {
        let mut m = AppModel::default();
        let _ = m.update(Message::PasswordChanged("unsaved-draft".into()));
        let _ = m.update(Message::PasswordLoaded(Ok("password-saved-in-nm".into())));
        assert_eq!(m.settings.password, "unsaved-draft");
    }

    #[test]
    fn loaded_password_fills_untouched_draft() {
        let mut m = AppModel::default();
        let _ = m.update(Message::PasswordLoaded(Ok("stored-password".into())));
        assert_eq!(m.settings.password, "stored-password");
    }

    #[test]
    fn toggle_password_visible_flips_the_flag() {
        let mut m = AppModel::default();
        assert!(!m.password_visible);
        let _ = m.update(Message::TogglePasswordVisible);
        assert!(m.password_visible);
        let _ = m.update(Message::TogglePasswordVisible);
        assert!(!m.password_visible);
    }

    #[test]
    fn opening_the_popup_resets_password_visibility() {
        let mut m = AppModel::default();
        m.core.set_main_window_id(Some(Id::unique()));
        m.password_visible = true;
        let _ = m.update(Message::TogglePopup);
        assert!(!m.password_visible);
    }

    #[test]
    fn silent_start_failure_detected() {
        let mut p = Some(0);
        assert!(!start_failed(&mut p, HotspotState::Starting));
        assert!(start_failed(&mut p, HotspotState::Off));
        assert_eq!(p, None);
        let mut p = Some(0);
        assert!(!start_failed(&mut p, HotspotState::Off));
        assert!(!start_failed(&mut p, HotspotState::Off));
        assert!(start_failed(&mut p, HotspotState::Off));
        let mut p = Some(0);
        assert!(!start_failed(&mut p, HotspotState::On));
        assert!(!start_failed(&mut p, HotspotState::Off));
    }
}
