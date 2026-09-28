use crate::config::Settings;
use std::collections::HashMap;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{proxy, Connection};

/// Name shown in NetworkManager; the profile is always identified by `PROFILE_UUID`.
pub const PROFILE_ID: &str = "COSMIC Hotspot";
const PROFILE_UUID: &str = "6f1c2c1e-6b0e-4c55-9a3e-5b1d7a0c9a10";
const DEVICE_TYPE_WIFI: u32 = 2;
const ACTIVE_ACTIVATING: u32 = 1;
const ACTIVE_ACTIVATED: u32 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotspotState { Off, Starting, On }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Client { pub ip: String, pub mac: String }

#[derive(Clone, Debug)]
pub struct Snapshot {
    pub state: HotspotState,
    pub clients: Vec<Client>,
    pub uplink_ok: bool,
    pub wifi_client_connected: bool,
}

pub fn needs_confirm(uplink_ok: bool, wifi_client_connected: bool) -> bool {
    !uplink_ok && wifi_client_connected
}

/// Lowercase MACs from `iw dev <if> station dump` lines `Station <mac> (on <if>)`.
pub fn parse_stations(out: &str) -> Vec<String> {
    out.lines()
        .filter_map(|l| l.strip_prefix("Station "))
        .filter_map(|r| r.split_whitespace().next())
        .map(|m| m.to_lowercase())
        .collect()
}

/// Stations associated with the AP, via `iw`; None if `iw` is missing or fails.
async fn iw_stations(iface: &str) -> Option<Vec<String>> {
    for bin in ["iw", "/usr/sbin/iw"] {
        if let Ok(o) = tokio::process::Command::new(bin).args(["dev", iface, "station", "dump"]).output().await {
            return o.status.success().then(|| parse_stations(&String::from_utf8_lossy(&o.stdout)));
        }
    }
    None
}

/// Parses `ip -4 neigh show dev <iface>`. STALE counts: an idle phone goes STALE but stays connected.
pub fn parse_neigh(out: &str) -> Vec<Client> {
    out.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            let ip = f.first()?;
            if ip.contains(':') { return None; } // IPv4 only
            let i = f.iter().position(|x| *x == "lladdr")?;
            let mac = f.get(i + 1)?;
            let st = f.last()?;
            matches!(*st, "REACHABLE" | "STALE" | "DELAY" | "PROBE")
                .then(|| Client { ip: (*ip).into(), mac: (*mac).into() })
        })
        .collect()
}

#[proxy(interface = "org.freedesktop.NetworkManager",
        default_service = "org.freedesktop.NetworkManager",
        default_path = "/org/freedesktop/NetworkManager")]
trait Nm {
    #[zbus(property)] fn active_connections(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn get_devices(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    fn activate_connection(&self, connection: &ObjectPath<'_>, device: &ObjectPath<'_>,
                           specific_object: &ObjectPath<'_>) -> zbus::Result<OwnedObjectPath>;
    fn deactivate_connection(&self, active: &ObjectPath<'_>) -> zbus::Result<()>;
}

#[proxy(interface = "org.freedesktop.NetworkManager.Settings",
        default_service = "org.freedesktop.NetworkManager",
        default_path = "/org/freedesktop/NetworkManager/Settings")]
trait NmSettings {
    fn get_connection_by_uuid(&self, uuid: &str) -> zbus::Result<OwnedObjectPath>;
    fn add_connection(&self, c: HashMap<String, HashMap<String, Value<'_>>>) -> zbus::Result<OwnedObjectPath>;
}

#[proxy(interface = "org.freedesktop.NetworkManager.Settings.Connection",
        default_service = "org.freedesktop.NetworkManager")]
trait NmConn {
    fn update(&self, c: HashMap<String, HashMap<String, Value<'_>>>) -> zbus::Result<()>;
    fn get_secrets(&self, setting: &str) -> zbus::Result<HashMap<String, HashMap<String, OwnedValue>>>;
}

#[proxy(interface = "org.freedesktop.NetworkManager.Connection.Active",
        default_service = "org.freedesktop.NetworkManager")]
trait NmActive {
    #[zbus(property)] fn uuid(&self) -> zbus::Result<String>;
    #[zbus(property, name = "Type")] fn type_(&self) -> zbus::Result<String>;
    #[zbus(property)] fn state(&self) -> zbus::Result<u32>;
}

#[proxy(interface = "org.freedesktop.NetworkManager.Device",
        default_service = "org.freedesktop.NetworkManager")]
trait NmDevice {
    #[zbus(property)] fn device_type(&self) -> zbus::Result<u32>;
    #[zbus(property)] fn interface(&self) -> zbus::Result<String>;
}

type Dict = HashMap<String, HashMap<String, Value<'static>>>;

fn kv(pairs: Vec<(&str, Value<'static>)>) -> HashMap<String, Value<'static>> {
    pairs.into_iter().map(|(k, v)| (k.to_string(), v)).collect()
}

fn profile_dict(s: &Settings, iface: &str) -> Dict {
    let mut d = Dict::new();
    d.insert("connection".into(), kv(vec![
        ("id", PROFILE_ID.to_string().into()),
        ("uuid", PROFILE_UUID.to_string().into()),
        ("type", "802-11-wireless".to_string().into()),
        ("interface-name", iface.to_string().into()),
        ("autoconnect", false.into()),
    ]));
    d.insert("802-11-wireless".into(), kv(vec![
        ("ssid", s.ssid.clone().into_bytes().into()),
        ("mode", "ap".to_string().into()),
        ("band", s.band.nm_value().to_string().into()),
    ]));
    d.insert("802-11-wireless-security".into(), kv(vec![
        ("key-mgmt", "wpa-psk".to_string().into()),
        ("psk", s.password.clone().into()),
        ("psk-flags", 0u32.into()),
        ("proto", vec!["rsn".to_string()].into()),
        ("pairwise", vec!["ccmp".to_string()].into()),
        ("group", vec!["ccmp".to_string()].into()),
    ]));
    d.insert("ipv4".into(), kv(vec![("method", "shared".to_string().into())]));
    d.insert("ipv6".into(), kv(vec![("method", "ignore".to_string().into())]));
    d
}

fn e<T: std::fmt::Display>(x: T) -> String { x.to_string() }

async fn wifi_device(c: &Connection) -> Result<(OwnedObjectPath, String), String> {
    let nm = NmProxy::new(c).await.map_err(e)?;
    for p in nm.get_devices().await.map_err(e)? {
        let d = NmDeviceProxy::builder(c).path(p.clone()).map_err(e)?.build().await.map_err(e)?;
        if d.device_type().await.map_err(e)? == DEVICE_TYPE_WIFI {
            return Ok((p, d.interface().await.map_err(e)?));
        }
    }
    Err(crate::fl!("err-no-wifi"))
}

/// Creates (fixed uuid) or updates the hotspot profile; returns its object path.
async fn ensure_profile(c: &Connection, s: &Settings, iface: &str) -> Result<OwnedObjectPath, String> {
    s.validate()?;
    let st = NmSettingsProxy::new(c).await.map_err(e)?;
    match st.get_connection_by_uuid(PROFILE_UUID).await {
        Ok(path) => {
            let conn = NmConnProxy::builder(c).path(path.clone()).map_err(e)?.build().await.map_err(e)?;
            conn.update(profile_dict(s, iface)).await.map_err(e)?;
            Ok(path)
        }
        Err(_) => st.add_connection(profile_dict(s, iface)).await.map_err(e),
    }
}

/// (path, uuid, type, state) of every active connection.
async fn actives(c: &Connection) -> Result<Vec<(OwnedObjectPath, String, String, u32)>, String> {
    let nm = NmProxy::new(c).await.map_err(e)?;
    let mut out = vec![];
    for p in nm.active_connections().await.map_err(e)? {
        let a = NmActiveProxy::builder(c).path(p.clone()).map_err(e)?.build().await.map_err(e)?;
        out.push((p, a.uuid().await.map_err(e)?, a.type_().await.map_err(e)?, a.state().await.map_err(e)?));
    }
    Ok(out)
}

/// Several activations of the profile can coexist (down/up): Activated > Activating > anything else (Off).
pub fn hotspot_state(states: impl IntoIterator<Item = u32>) -> HotspotState {
    let v: Vec<u32> = states.into_iter().collect();
    match pick_active(&v) {
        Some(i) if v[i] == ACTIVE_ACTIVATED => HotspotState::On,
        Some(_) => HotspotState::Starting,
        None => HotspotState::Off,
    }
}

/// Index of the entry to use or tear down: Activated > Activating; None if there are only others.
pub fn pick_active(states: &[u32]) -> Option<usize> {
    [ACTIVE_ACTIVATED, ACTIVE_ACTIVATING].iter().find_map(|w| states.iter().position(|s| s == w))
}

const DEACTIVATE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(150);
const DEACTIVATE_MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// True if `PROFILE_UUID` is among the given uuids.
fn still_active<'a>(uuids: impl IntoIterator<Item = &'a str>) -> bool {
    uuids.into_iter().any(|u| u == PROFILE_UUID)
}

/// Waits for the hotspot profile to be fully gone from NetworkManager's active connections
/// before a new activation on the same device, so the previous activation's dnsmasq can't
/// be orphaned (seen in practice: PPID=1, holding 10.42.0.1:53). Best effort: never fails
/// the call; gives up after DEACTIVATE_MAX_WAIT or if D-Bus fails.
async fn wait_until_deactivated() {
    let deadline = tokio::time::Instant::now() + DEACTIVATE_MAX_WAIT;
    loop {
        let Ok(c) = Connection::system().await else { return };
        let Ok(act) = actives(&c).await else { return };
        if !still_active(act.iter().map(|a| a.1.as_str())) {
            return;
        }
        if tokio::time::Instant::now() >= deadline {
            return;
        }
        tokio::time::sleep(DEACTIVATE_POLL_INTERVAL).await;
    }
}

pub async fn snapshot() -> Result<Snapshot, String> {
    let c = Connection::system().await.map_err(e)?;
    let act = actives(&c).await?;
    let state = hotspot_state(act.iter().filter(|a| a.1 == PROFILE_UUID).map(|a| a.3));
    let uplink_ok = act.iter().any(|a| a.2 == "802-3-ethernet" && a.3 == ACTIVE_ACTIVATED);
    let wifi_client_connected =
        act.iter().any(|a| a.2 == "802-11-wireless" && a.1 != PROFILE_UUID && a.3 == ACTIVE_ACTIVATED);
    let clients = if state == HotspotState::On {
        let (_, iface) = wifi_device(&c).await?;
        let out = tokio::process::Command::new("ip")
            .args(["-4", "neigh", "show", "dev", &iface])
            .output().await.map_err(e)?;
        let neigh = parse_neigh(&String::from_utf8_lossy(&out.stdout));
        match iw_stations(&iface).await {
            Some(macs) => macs.into_iter().map(|mac| {
                let ip = neigh.iter().find(|n| n.mac.eq_ignore_ascii_case(&mac)).map(|n| n.ip.clone()).unwrap_or_default();
                Client { mac, ip }
            }).collect(),
            None => neigh,
        }
    } else { vec![] };
    Ok(Snapshot { state, clients, uplink_ok, wifi_client_connected })
}

pub async fn up(s: &Settings) -> Result<(), String> {
    let c = Connection::system().await.map_err(e)?;
    let (dev, iface) = wifi_device(&c).await?;
    let prof = ensure_profile(&c, s, &iface).await?;
    let root = ObjectPath::try_from("/").map_err(e)?;
    NmProxy::new(&c).await.map_err(e)?
        .activate_connection(&prof, &dev, &root).await.map_err(e)?;
    Ok(())
}

pub async fn down() -> Result<(), String> {
    let c = Connection::system().await.map_err(e)?;
    let mine: Vec<_> = actives(&c).await?.into_iter().filter(|a| a.1 == PROFILE_UUID).collect();
    let states: Vec<u32> = mine.iter().map(|a| a.3).collect();
    if let Some(i) = pick_active(&states) {
        NmProxy::new(&c).await.map_err(e)?.deactivate_connection(&mine[i].0).await.map_err(e)?;
    }
    Ok(())
}

pub async fn load_password() -> Result<String, String> {
    let c = Connection::system().await.map_err(e)?;
    let path = NmSettingsProxy::new(&c).await.map_err(e)?
        .get_connection_by_uuid(PROFILE_UUID).await.map_err(|_| crate::fl!("err-no-profile"))?;
    let conn = NmConnProxy::builder(&c).path(path).map_err(e)?.build().await.map_err(e)?;
    let sec = conn.get_secrets("802-11-wireless-security").await.map_err(e)?;
    let v = sec.get("802-11-wireless-security").and_then(|m| m.get("psk")).ok_or_else(|| crate::fl!("err-no-secret"))?;
    String::try_from(v.try_clone().map_err(e)?).map_err(e)
}

pub async fn apply_settings(s: &Settings) -> Result<(), String> {
    let was_on = snapshot().await?.state != HotspotState::Off;
    if was_on {
        down().await?;
        wait_until_deactivated().await;
    }
    up_or_save(s, was_on).await
}

async fn up_or_save(s: &Settings, activate: bool) -> Result<(), String> {
    if activate { return up(s).await; }
    let c = Connection::system().await.map_err(e)?;
    let (_, iface) = wifi_device(&c).await?;
    ensure_profile(&c, s, &iface).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirm_only_when_no_cable_and_wifi_client() {
        assert!(needs_confirm(false, true));
        assert!(!needs_confirm(true, true));
        assert!(!needs_confirm(false, false));
        assert!(!needs_confirm(true, false));
    }

    #[test]
    fn parses_neigh_output() {
        let out = "\
10.42.0.23 lladdr aa:bb:cc:dd:ee:01 REACHABLE
10.42.0.24 lladdr aa:bb:cc:dd:ee:02 STALE
10.42.0.25  FAILED
10.42.0.26 lladdr aa:bb:cc:dd:ee:03 DELAY
fe80::1 lladdr aa:bb:cc:dd:ee:04 REACHABLE
";
        let c = parse_neigh(out);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], Client { ip: "10.42.0.23".into(), mac: "aa:bb:cc:dd:ee:01".into() });
        assert_eq!(c[2].ip, "10.42.0.26");
    }

    #[test]
    fn parses_station_dump() {
        let out = "Station 44:89:6d:ef:27:9e (on wlp8s0)\n\tinactive time:\t100 ms\n\tsignal:\t-50 dBm\nStation aa:bb:cc:dd:ee:02 (on wlp8s0)\n\tinactive time:\t5 ms\n";
        assert_eq!(parse_stations(out), vec!["44:89:6d:ef:27:9e", "aa:bb:cc:dd:ee:02"]);
    }

    #[test]
    fn parses_empty_station_dump() {
        assert!(parse_stations("").is_empty());
    }

    #[test]
    fn station_mac_lowercased() {
        assert_eq!(parse_stations("Station AA:BB:CC:DD:EE:0F (on wlan0)\n"), vec!["aa:bb:cc:dd:ee:0f"]);
    }

    #[test]
    fn hotspot_state_prefers_activated_then_activating() {
        assert_eq!(hotspot_state([4, ACTIVE_ACTIVATED]), HotspotState::On);
        assert_eq!(hotspot_state([4, ACTIVE_ACTIVATING]), HotspotState::Starting);
        assert_eq!(hotspot_state([4]), HotspotState::Off);
        assert_eq!(hotspot_state([]), HotspotState::Off);
    }

    #[test]
    fn pick_active_prefers_activated() {
        assert_eq!(pick_active(&[3, ACTIVE_ACTIVATED]), Some(1));
        assert_eq!(pick_active(&[ACTIVE_ACTIVATING, 3]), Some(0));
        assert_eq!(pick_active(&[3]), None);
        assert_eq!(pick_active(&[]), None);
    }

    #[test]
    fn still_active_matches_the_profile_uuid_not_its_name() {
        assert!(still_active([PROFILE_UUID, "other"]));
        assert!(!still_active([PROFILE_ID]), "only the uuid identifies the profile, never its name");
        assert!(!still_active(["other"]));
        assert!(!still_active([]));
    }
}
