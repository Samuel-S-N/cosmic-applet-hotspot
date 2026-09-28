// SPDX-License-Identifier: MPL-2.0

use crate::app::{AppModel, Message, Tab};
use crate::config::Band;
use crate::fl;
use crate::nm::{self, HotspotState};
use cosmic::prelude::*;
use cosmic::widget;

#[derive(Debug, PartialEq)]
pub enum ToggleAction {
    TurnOff,
    TurnOn,
    AskConfirm,
    /// No network name/password yet: send the user to the settings tab instead.
    NeedsSetup,
}

pub fn toggle_action(want_on: bool, configured: bool, snap: &nm::Snapshot) -> ToggleAction {
    if !want_on {
        return ToggleAction::TurnOff;
    }
    if !configured {
        return ToggleAction::NeedsSetup;
    }
    if nm::needs_confirm(snap.uplink_ok, snap.wifi_client_connected) {
        ToggleAction::AskConfirm
    } else {
        ToggleAction::TurnOn
    }
}

const BANDS: [&str; 2] = ["2.4 GHz", "5 GHz"];

pub fn view(m: &AppModel) -> Element<'_, Message> {
    let tabs = widget::row::with_capacity(2)
        .spacing(8)
        .push(
            widget::button::text(fl!("tab-main"))
                .class(tab_class(m.tab == Tab::Main))
                .on_press(Message::Tab(Tab::Main)),
        )
        .push(
            widget::button::text(fl!("tab-settings"))
                .class(tab_class(m.tab == Tab::Settings))
                .on_press(Message::Tab(Tab::Settings)),
        );
    let body = match m.tab {
        Tab::Main => main_tab(m),
        Tab::Settings => settings_tab(m),
    };
    let mut col = widget::column::with_capacity(8).spacing(12).padding(12).push(tabs).push(body);
    if let Some(e) = &m.error {
        col = col.push(widget::text::body(e.clone()).class(cosmic::style::Text::Color(
            cosmic::theme::active().cosmic().destructive_text_color().into(),
        )));
    }
    m.core.applet.popup_container(opaque_card(col)).into()
}

const ICON_OFF: &[u8] = include_bytes!("../resources/icons/hotspot-off-symbolic.svg");
const ICON_ON: &[u8] = include_bytes!("../resources/icons/hotspot-on-symbolic.svg");

/// Panel button: outlined icon when off, filled icon when on/starting.
pub fn panel_button(m: &AppModel) -> Element<'_, Message> {
    let on = matches!(&m.snap, Some(s) if s.state != HotspotState::Off);
    let handle = cosmic::widget::icon::from_svg_bytes(if on { ICON_ON } else { ICON_OFF })
        .symbolic(true);
    m.core.applet.icon_button_from_handle(handle).on_press(Message::TogglePopup).into()
}

/// Opaque card that covers `popup_container`'s theme background. libcosmic paints
/// `cosmic.background(theme.transparent)`, which shows the windows behind the popup
/// when the system blur flag is on; the system Wi-Fi popup is opaque, so use the
/// non-transparent background here.
fn opaque_card<'a>(content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    use cosmic::iced::{Border, Color, Length, Shadow};
    cosmic::iced::widget::Container::new(content)
        .width(Length::Fill)
        .style(|theme: &cosmic::Theme| {
            let cosmic = theme.cosmic();
            let bg = cosmic.background(false);
            cosmic::iced::widget::container::Style {
                text_color: Some(bg.on.into()),
                background: Some(Color::from(bg.base).into()),
                border: Border {
                    radius: cosmic.corner_radii.radius_m.into(),
                    width: 1.0,
                    color: bg.divider.into(),
                },
                shadow: Shadow::default(),
                icon_color: Some(bg.on.into()),
                snap: true,
            }
        })
        .into()
}

fn tab_class(active: bool) -> cosmic::theme::Button {
    if active { cosmic::theme::Button::Suggested } else { cosmic::theme::Button::Standard }
}

fn main_tab(m: &AppModel) -> Element<'_, Message> {
    let state = m.snap.as_ref().map(|s| s.state).unwrap_or(HotspotState::Off);
    let mut toggler = widget::toggler(state != HotspotState::Off);
    if state != HotspotState::Starting && !m.busy {
        toggler = toggler.on_toggle(Message::Toggle);
    }
    let status = match state {
        HotspotState::Off => fl!("status-off"),
        HotspotState::Starting => fl!("status-starting"),
        HotspotState::On => fl!("status-on", ssid = m.settings.ssid.clone()),
    };
    let mut col = widget::column::with_capacity(8)
        .spacing(12)
        .push(widget::settings::item(fl!("hotspot"), toggler))
        .push(widget::text::body(status));
    if m.confirm {
        col = col
            .push(widget::text::body(
                fl!("confirm-warning"),
            ))
            .push(
                widget::row::with_capacity(2)
                    .spacing(8)
                    .push(widget::button::suggested(fl!("continue")).on_press(Message::ConfirmYes))
                    .push(widget::button::standard(fl!("cancel")).on_press(Message::ConfirmNo)),
            );
    }
    match m.snap.as_ref().map(|s| &s.clients) {
        Some(c) if !c.is_empty() => {
            for c in c {
                let line = if c.ip.is_empty() { c.mac.clone() } else { format!("{} — {}", c.ip, c.mac) };
                col = col.push(widget::text::body(line));
            }
        }
        _ => col = col.push(widget::text::body(fl!("no-clients"))),
    }
    col.into()
}

fn settings_tab(m: &AppModel) -> Element<'_, Message> {
    let band = match m.settings.band {
        Band::Ghz24 => 0,
        Band::Ghz5 => 1,
    };
    let idle = if m.settings.idle_minutes == 0 { String::new() } else { m.settings.idle_minutes.to_string() };
    widget::column::with_capacity(8)
        .spacing(12)
        .push(widget::text_input(fl!("ssid"), &m.settings.ssid).on_input(Message::SsidChanged))
        .push({
            let eye_icon = if m.password_visible { "image-red-eye-symbolic" } else { "document-properties-symbolic" };
            let eye_button = widget::button::custom(widget::icon::from_name(eye_icon).size(16))
                .class(cosmic::theme::Button::Icon)
                .on_press(Message::TogglePasswordVisible)
                .padding(8);
            let mut input = widget::text_input(fl!("password"), &m.settings.password)
                .on_input(Message::PasswordChanged)
                .trailing_icon(eye_button.into());
            if !m.password_visible {
                input = input.password();
            }
            input
        })
        .push(widget::dropdown(&BANDS, Some(band), Message::BandChanged))
        .push(
            widget::text_input(fl!("idle-placeholder"), idle)
                .on_input(Message::IdleChanged),
        )
        .push(widget::button::suggested(fl!("save")).on_press(Message::Save))
        .push(widget::text::caption(fl!("save-hint")))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nm::{HotspotState, Snapshot};

    #[test]
    fn panel_icons_are_embedded_svgs() {
        for bytes in [ICON_OFF, ICON_ON] {
            let s = std::str::from_utf8(bytes).expect("utf8");
            assert!(s.contains("<svg") && s.contains("viewBox=\"0 0 16 16\""));
        }
        assert!(std::str::from_utf8(ICON_ON).unwrap().contains("evenodd"));
    }

    fn snap(uplink_ok: bool, wifi: bool) -> Snapshot {
        Snapshot { state: HotspotState::Off, clients: vec![], uplink_ok, wifi_client_connected: wifi }
    }

    #[test]
    fn toggle_decisions() {
        assert_eq!(toggle_action(false, true, &snap(false, true)), ToggleAction::TurnOff);
        assert_eq!(toggle_action(true, true, &snap(true, true)), ToggleAction::TurnOn);
        assert_eq!(toggle_action(true, true, &snap(false, false)), ToggleAction::TurnOn);
        assert_eq!(toggle_action(true, true, &snap(false, true)), ToggleAction::AskConfirm);
    }

    #[test]
    fn turning_on_unconfigured_needs_setup_first() {
        // Setup comes before the no-cable warning: there is nothing to start yet.
        assert_eq!(toggle_action(true, false, &snap(true, false)), ToggleAction::NeedsSetup);
        assert_eq!(toggle_action(true, false, &snap(false, true)), ToggleAction::NeedsSetup);
        // Turning off never needs setup.
        assert_eq!(toggle_action(false, false, &snap(true, false)), ToggleAction::TurnOff);
    }
}
