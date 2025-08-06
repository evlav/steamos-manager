/*
 * Copyright © 2023 Collabora Ltd.
 * Copyright © 2024 Valve Software
 *
 * SPDX-License-Identifier: MIT
 */

use anyhow::{anyhow, Result};
use clap::{ArgAction, Parser, Subcommand};
use itertools::Itertools;
use nix::time::{clock_gettime, ClockId};
use std::collections::HashMap;
use std::io::Cursor;
use steamos_manager::cec::HdmiCecState;
use steamos_manager::hardware::{FactoryResetKind, FanControlState};
use steamos_manager::power::{CPUBoostState, CPUScalingGovernor};
use steamos_manager::proxy::{
    AmbientLightSensor1Proxy, BatteryChargeLimit1Proxy, CpuBoost1Proxy, CpuScaling1Proxy,
    FactoryReset1Proxy, FanControl1Proxy, GpuPerformanceLevel1Proxy, GpuPowerProfile1Proxy,
    HdmiCec1Proxy, LowPowerMode1Proxy, Manager2Proxy, PerformanceProfile1Proxy, ScreenReader1Proxy,
    SessionManagement1Proxy, Storage1Proxy, TdpLimit1Proxy, UpdateBios1Proxy, UpdateDock1Proxy,
    WifiDebug1Proxy, WifiDebugDump1Proxy, WifiPowerManagement1Proxy,
};
use steamos_manager::screenreader::{ScreenReaderAction, ScreenReaderMode};
use steamos_manager::session::LoginMode;
use steamos_manager::wifi::{WifiBackend, WifiDebugMode, WifiPowerManagement};
use zbus::fdo::{IntrospectableProxy, PropertiesProxy};
use zbus::{zvariant, Connection};
use zbus_xml::Node;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get all properties
    GetAllProperties,

    /// Get luminance sensor calibration gain
    GetAlsCalibrationGain,

    /// Set the fan control state
    SetFanControlState {
        /// Valid options are `bios`, `os`
        state: FanControlState,
    },

    /// Get the fan control state
    GetFanControlState,

    /// Get the available CPU scaling governors supported on this device
    GetAvailableCpuScalingGovernors,

    /// Get the current CPU governor
    GetCpuScalingGovernor,

    /// Set the current CPU Scaling governor
    SetCpuScalingGovernor {
        /// Valid governors are get-cpu-governors.
        governor: CPUScalingGovernor,
    },

    /// Get the current CPU boost state
    GetCpuBoostState,

    /// Set the CPU boost state
    SetCpuBoostState {
        /// Valid states are `enabled`, `disabled`
        state: CPUBoostState,
    },

    /// Get the GPU power profiles supported on this device
    GetAvailableGPUPowerProfiles,

    /// Get the current GPU power profile
    GetGPUPowerProfile,

    /// Set the GPU Power profile
    SetGPUPowerProfile {
        /// Valid profiles can be obtained from get-gpu-power-profiles.
        profile: String,
    },

    /// Set the GPU performance level
    SetGPUPerformanceLevel {
        /// Valid levels are `auto`, `low`, `high`, `manual`, `profile_peak`
        level: String,
    },

    /// Get the GPU performance level
    GetGPUPerformanceLevel,

    /// Set the GPU clock value manually. Only works when performance level is set to `manual`
    SetManualGPUClock {
        /// GPU clock frequency in MHz
        freq: u32,
    },

    /// Get the GPU clock frequency, in MHz. Only works when performance level is set to `manual`
    GetManualGPUClock,

    /// Get the maximum allowed GPU clock frequency for the `manual` performance level
    GetManualGPUClockMax,

    /// Get the minimum allowed GPU clock frequency for the `manual` performance level
    GetManualGPUClockMin,

    /// Set the TDP limit
    SetTDPLimit {
        /// TDP limit, in W
        limit: u32,
    },

    /// Get the TDP limit
    GetTDPLimit,

    /// Get the maximum allowed TDP limit
    GetTDPLimitMax,

    /// Get the minimum allowed TDP limit
    GetTDPLimitMin,

    /// Get the performance profiles supported on this device
    GetAvailablePerformanceProfiles,

    /// Get the current performance profile
    GetPerformanceProfile,

    /// Set the performance profile
    SetPerformanceProfile {
        /// Valid profiles can be found using get-available-performance-profiles.
        profile: String,
    },

    /// Get the suggested default performance profile
    SuggestedDefaultPerformanceProfile,

    /// Set the Wi-Fi backend, if possible
    SetWifiBackend {
        /// Supported backends are `iwd`, `wpa_supplicant`
        backend: WifiBackend,
    },

    /// Get the Wi-Fi backend
    GetWifiBackend,

    /// Set Wi-Fi debug mode, if possible
    SetWifiDebugMode {
        /// Valid modes are `off` or `tracing`
        mode: WifiDebugMode,
        /// The size of the debug buffer, in bytes
        buffer: Option<u32>,
    },

    /// Get Wi-Fi debug mode
    GetWifiDebugMode,

    /// Capture the current Wi-Fi debug trace
    CaptureWifiDebugTraceOutput,

    /// Set the Wi-Fi power management state
    SetWifiPowerManagementState {
        /// Valid modes are `enabled`, `disabled`
        state: WifiPowerManagement,
    },

    /// Get the Wi-Fi power management state
    GetWifiPowerManagementState,

    /// Generate a Wi-Fi debug dump
    GenerateWifiDebugDump,

    /// Get the state of HDMI-CEC support
    GetHdmiCecState,

    /// Set the state of HDMI-CEC support
    SetHdmiCecState {
        /// Valid modes are `disabled`, `control-only`, `control-and-wake`
        state: HdmiCecState,
    },

    /// List active low power download mode handles
    ListLowPowerDownloadModeHandles,

    /// Update the BIOS, if possible
    UpdateBios,

    /// Update the dock, if possible
    UpdateDock,

    /// Trim applicable drives
    TrimDevices,

    /// Factory reset the os/user partitions
    PrepareFactoryReset {
        /// Valid kind(s) are `user`, `os`, `all`
        kind: FactoryResetKind,
    },

    /// Get the maximum charge level set for the battery
    GetMaxChargeLevel,

    /// Set the maximum charge level set for the battery
    SetMaxChargeLevel {
        /// Valid levels are 1 - 100, or -1 to reset to default
        level: i32,
    },

    /// Get the recommended minimum for a charge level limit
    SuggestedMinimumChargeLimit,

    /// Reload the configuration from disk
    ReloadConfig,

    /// Get the model and variant of this device, if known
    GetDeviceModel,

    /// Get whether screen reader is enabled or not.
    GetScreenReaderEnabled,

    /// Enable or disable the screen reader
    SetScreenReaderEnabled {
        #[arg(action = ArgAction::Set, required = true)]
        enable: bool,
    },

    /// Get screen reader rate
    GetScreenReaderRate,

    /// Set screen reader rate
    SetScreenReaderRate {
        /// Valid rates between 0.0 for slowest and 100.0 for fastest.
        rate: f64,
    },

    /// Get screen reader pitch
    GetScreenReaderPitch,

    /// Set screen reader pitch
    SetScreenReaderPitch {
        /// Valid pitches between 0.0 for lowest and 10.0 for highest.
        pitch: f64,
    },

    /// Get screen reader volume
    GetScreenReaderVolume,

    /// Set screen reader volume
    SetScreenReaderVolume {
        /// Valid volume between 0.0 for off, and 10.0 for loudest.
        volume: f64,
    },

    /// Get screen reader mode
    GetScreenReaderMode,

    /// Set screen reader mode
    SetScreenReaderMode {
        /// Valid modes are `browse`, `focus`
        mode: ScreenReaderMode,
    },

    /// Get screen reader known locales
    GetScreenReaderLocales,

    /// Get screen reader voices for given locale
    GetScreenReaderVoicesForLocale {
        /// Valid locales can be found using get-screen-reader-locales.
        locale: String,
    },

    /// Get screen reader voice
    GetScreenReaderVoice,

    /// Set screen reader voice
    SetScreenReaderVoice {
        /// The voice name to use for screen reader. Valid voices can be found using get-screen-reader-voices-for-locale
        voice: String,
    },

    /// Trigger screen reader action
    TriggerScreenReaderAction {
        /// Valid actions are
        /// `stop_talking`,
        /// `read_next_word`,
        /// `read_previous_word`,
        /// `read_next_item`,
        /// `read_previous_item`,
        /// `move_to_next_landmark`,
        /// `move_to_previous_landmark`,
        /// `move_to_next_heading`,
        /// `move_to_previous_heading`,
        /// `toggle_mode`
        action: ScreenReaderAction,
    },

    /// Switch from the current session into desktop mode
    SwitchToDesktopMode,

    /// Switch from the current session into game mode
    SwitchToGameMode,

    /// Switch from the current session into the given login mode
    SwitchToLoginMode {
        /// Valid login modes are `game` and `desktop`
        mode: LoginMode,
    },

    /// Set the default desktop session for the desktop login mode
    SetDefaultDesktopSession {
        /// The name of the session. Valid sessions can be
        /// obtained using get-valid-desktop-sessions.
        session: String,
    },

    /// Get the default desktop session
    GetDefaultDesktopSession,

    /// Set the default login mode
    SetDefaultLoginMode {
        /// Valid login modes are `game` and `desktop`
        mode: LoginMode,
    },

    /// Get the default login mode
    GetDefaultLoginMode,

    /// Get a list of the valid desktop sessions
    GetValidDesktopSessions,

    #[command(hide = true)]
    // This is an internal-only command that isn't useful for end-users
    CleanTemporarySessions,
}

async fn get_all_properties(conn: &Connection) -> Result<()> {
    let proxy = IntrospectableProxy::builder(conn)
        .destination("com.steampowered.SteamOSManager1")?
        .path("/com/steampowered/SteamOSManager1")?
        .build()
        .await?;
    let introspection = proxy.introspect().await?;
    let introspection = Node::from_reader(Cursor::new(introspection))?;

    let properties_proxy = PropertiesProxy::new(
        conn,
        "com.steampowered.SteamOSManager1",
        "/com/steampowered/SteamOSManager1",
    )
    .await?;

    let mut properties = HashMap::new();
    for interface in introspection.interfaces() {
        let name = match interface.name() {
            name if name
                .as_str()
                .starts_with("com.steampowered.SteamOSManager1") =>
            {
                name
            }
            _ => continue,
        };
        properties.extend(properties_proxy.get_all(name).await?);
    }
    for key in properties.keys().sorted() {
        let value = &properties[key];
        let val = &**value;
        println!("{key}: {val}");
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<()> {
    // This is a command-line utility that calls api using dbus

    // First set up which command line arguments we support
    let args = Args::parse();

    // Then get a connection to the service
    let conn = Connection::session().await?;

    // Then process arguments
    match &args.command {
        Commands::GetAllProperties => {
            get_all_properties(&conn).await?;
        }
        Commands::GetAlsCalibrationGain => {
            let proxy = AmbientLightSensor1Proxy::new(&conn).await?;
            let gain = proxy.als_calibration_gain().await?;
            let gains = gain.into_iter().map(|g| g.to_string()).join(", ");
            println!("ALS calibration gain: {gains}");
        }
        Commands::SetFanControlState { state } => {
            let proxy = FanControl1Proxy::new(&conn).await?;
            proxy.set_fan_control_state(*state as u32).await?;
        }
        Commands::GetFanControlState => {
            let proxy = FanControl1Proxy::new(&conn).await?;
            let state = proxy.fan_control_state().await?;
            match FanControlState::try_from(state) {
                Ok(s) => println!("Fan control state: {s}"),
                Err(_) => println!("Got unknown value {state} from backend"),
            }
        }
        Commands::GetAvailableCpuScalingGovernors => {
            let proxy = CpuScaling1Proxy::new(&conn).await?;
            let governors = proxy.available_cpu_scaling_governors().await?;
            println!("Governors:\n");
            for name in governors {
                println!("{name}");
            }
        }
        Commands::GetCpuScalingGovernor => {
            let proxy = CpuScaling1Proxy::new(&conn).await?;
            let governor = proxy.cpu_scaling_governor().await?;
            let governor_type = CPUScalingGovernor::try_from(governor.as_str());
            match governor_type {
                Ok(_) => {
                    println!("CPU Governor: {governor}");
                }
                Err(_) => {
                    println!("Unknown CPU governor or unable to get type from {governor}");
                }
            }
        }
        Commands::SetCpuScalingGovernor { governor } => {
            let proxy = CpuScaling1Proxy::new(&conn).await?;
            proxy
                .set_cpu_scaling_governor(governor.to_string().as_str())
                .await?;
        }
        Commands::GetCpuBoostState => {
            let proxy = CpuBoost1Proxy::new(&conn).await?;
            let state = proxy.cpu_boost_state().await?;
            match CPUBoostState::try_from(state) {
                Ok(s) => println!("CPU Boost State: {s}"),
                Err(_) => println!("Got unknown value {state} from backend"),
            }
        }
        Commands::SetCpuBoostState { state } => {
            let proxy = CpuBoost1Proxy::new(&conn).await?;
            proxy.set_cpu_boost_state(*state as u32).await?;
        }
        Commands::GetAvailableGPUPowerProfiles => {
            let proxy = GpuPowerProfile1Proxy::new(&conn).await?;
            let profiles = proxy.available_gpu_power_profiles().await?;
            println!("Profiles:\n");
            for name in profiles.into_iter().sorted() {
                println!("- {name}");
            }
        }
        Commands::GetGPUPowerProfile => {
            let proxy = GpuPowerProfile1Proxy::new(&conn).await?;
            let profile = proxy.gpu_power_profile().await?;
            println!("GPU Power Profile: {profile}");
        }
        Commands::SetGPUPowerProfile { profile } => {
            let proxy = GpuPowerProfile1Proxy::new(&conn).await?;
            proxy
                .set_gpu_power_profile(profile.to_string().as_str())
                .await?;
        }
        Commands::SetGPUPerformanceLevel { level } => {
            let proxy = GpuPerformanceLevel1Proxy::new(&conn).await?;
            proxy
                .set_gpu_performance_level(level.to_string().as_str())
                .await?;
        }
        Commands::GetGPUPerformanceLevel => {
            let proxy = GpuPerformanceLevel1Proxy::new(&conn).await?;
            let level = proxy.gpu_performance_level().await?;
            println!("GPU performance level: {level}");
        }
        Commands::SetManualGPUClock { freq } => {
            let proxy = GpuPerformanceLevel1Proxy::new(&conn).await?;
            proxy.set_manual_gpu_clock(*freq).await?;
        }
        Commands::GetManualGPUClock => {
            let proxy = GpuPerformanceLevel1Proxy::new(&conn).await?;
            let clock = proxy.manual_gpu_clock().await?;
            println!("Manual GPU Clock: {clock}");
        }
        Commands::GetManualGPUClockMax => {
            let proxy = GpuPerformanceLevel1Proxy::new(&conn).await?;
            let value = proxy.manual_gpu_clock_max().await?;
            println!("Manual GPU Clock Max: {value}");
        }
        Commands::GetManualGPUClockMin => {
            let proxy = GpuPerformanceLevel1Proxy::new(&conn).await?;
            let value = proxy.manual_gpu_clock_min().await?;
            println!("Manual GPU Clock Min: {value}");
        }
        Commands::GetAvailablePerformanceProfiles => {
            let proxy = PerformanceProfile1Proxy::new(&conn).await?;
            let profiles = proxy.available_performance_profiles().await?;
            println!("Profiles:\n");
            for name in profiles.into_iter().sorted() {
                println!("- {name}");
            }
        }
        Commands::GetPerformanceProfile => {
            let proxy = PerformanceProfile1Proxy::new(&conn).await?;
            let profile = proxy.performance_profile().await?;
            println!("Performance Profile: {profile}");
        }
        Commands::SetPerformanceProfile { profile } => {
            let proxy = PerformanceProfile1Proxy::new(&conn).await?;
            proxy.set_performance_profile(profile.as_str()).await?;
        }
        Commands::SuggestedDefaultPerformanceProfile => {
            let proxy = PerformanceProfile1Proxy::new(&conn).await?;
            let profile = proxy.suggested_default_performance_profile().await?;
            println!("Suggested Default Performance Profile: {profile}");
        }
        Commands::SetTDPLimit { limit } => {
            let proxy = TdpLimit1Proxy::new(&conn).await?;
            proxy.set_tdp_limit(*limit).await?;
        }
        Commands::GetTDPLimit => {
            let proxy = TdpLimit1Proxy::new(&conn).await?;
            let limit = proxy.tdp_limit().await?;
            println!("TDP limit: {limit}");
        }
        Commands::GetTDPLimitMax => {
            let proxy = TdpLimit1Proxy::new(&conn).await?;
            let value = proxy.tdp_limit_max().await?;
            println!("TDP limit max: {value}");
        }
        Commands::GetTDPLimitMin => {
            let proxy = TdpLimit1Proxy::new(&conn).await?;
            let value = proxy.tdp_limit_min().await?;
            println!("TDP limit min: {value}");
        }
        Commands::SetWifiBackend { backend } => {
            let proxy = WifiDebug1Proxy::new(&conn).await?;
            proxy.set_wifi_backend(backend.to_string().as_str()).await?;
        }
        Commands::GetWifiBackend => {
            let proxy = WifiDebug1Proxy::new(&conn).await?;
            let backend = proxy.wifi_backend().await?;
            match WifiBackend::try_from(backend.as_str()) {
                Ok(be) => println!("Wi-Fi backend: {be}"),
                Err(_) => println!("Got unknown value {backend} from backend"),
            }
        }
        Commands::SetWifiDebugMode { mode, buffer } => {
            let proxy = WifiDebug1Proxy::new(&conn).await?;
            let mut options = HashMap::<&str, &zvariant::Value<'_>>::new();
            let buffer_size;
            if let Some(size) = buffer {
                buffer_size = Some(zvariant::Value::U32(*size));
                options.insert("buffer_size", buffer_size.as_ref().unwrap());
            }
            proxy.set_wifi_debug_mode(*mode as u32, options).await?;
        }
        Commands::GetWifiDebugMode => {
            let proxy = WifiDebug1Proxy::new(&conn).await?;
            let mode = proxy.wifi_debug_mode_state().await?;
            match WifiDebugMode::try_from(mode) {
                Ok(m) => println!("Wi-Fi debug mode: {m}"),
                Err(_) => println!("Got unknown value {mode} from backend"),
            }
        }
        Commands::CaptureWifiDebugTraceOutput => {
            let proxy = WifiDebugDump1Proxy::new(&conn).await?;
            let path = proxy.generate_debug_dump().await?;
            println!("{path}");
        }
        Commands::SetWifiPowerManagementState { state } => {
            let proxy = WifiPowerManagement1Proxy::new(&conn).await?;
            proxy.set_wifi_power_management_state(*state as u32).await?;
        }
        Commands::GetWifiPowerManagementState => {
            let proxy = WifiPowerManagement1Proxy::new(&conn).await?;
            let state = proxy.wifi_power_management_state().await?;
            match WifiPowerManagement::try_from(state) {
                Ok(s) => println!("Wi-Fi power management state: {s}"),
                Err(_) => println!("Got unknown value {state} from backend"),
            }
        }
        Commands::GenerateWifiDebugDump => {
            let proxy = WifiDebugDump1Proxy::new(&conn).await?;
            let path = proxy.generate_debug_dump().await?;
            println!("{path}");
        }
        Commands::SetHdmiCecState { state } => {
            let proxy = HdmiCec1Proxy::new(&conn).await?;
            proxy.set_hdmi_cec_state(*state as u32).await?;
        }
        Commands::GetHdmiCecState => {
            let proxy = HdmiCec1Proxy::new(&conn).await?;
            let state = proxy.hdmi_cec_state().await?;
            match HdmiCecState::try_from(state) {
                Ok(s) => println!("HDMI-CEC state: {}", s.to_human_readable()),
                Err(_) => println!("Got unknown value {state} from backend"),
            }
        }
        Commands::ListLowPowerDownloadModeHandles => {
            let proxy = LowPowerMode1Proxy::new(&conn).await?;
            let handles: HashMap<String, u32> = proxy.list_download_mode_handles().await?;
            for (identifier, count) in handles.into_iter().sorted() {
                println!("{identifier}: {count}");
            }
        }
        Commands::UpdateBios => {
            let proxy = UpdateBios1Proxy::new(&conn).await?;
            let _ = proxy.update_bios().await?;
        }
        Commands::UpdateDock => {
            let proxy = UpdateDock1Proxy::new(&conn).await?;
            let _ = proxy.update_dock().await?;
        }
        Commands::PrepareFactoryReset { kind } => {
            let proxy = FactoryReset1Proxy::new(&conn).await?;
            let _ = proxy.prepare_factory_reset(*kind as u32).await?;
        }
        Commands::TrimDevices => {
            let proxy = Storage1Proxy::new(&conn).await?;
            let _ = proxy.trim_devices().await?;
        }
        Commands::GetMaxChargeLevel => {
            let proxy = BatteryChargeLimit1Proxy::new(&conn).await?;
            let level = proxy.max_charge_level().await?;
            println!("Max charge level: {level}");
        }
        Commands::SetMaxChargeLevel { level } => {
            let proxy = BatteryChargeLimit1Proxy::new(&conn).await?;
            proxy.set_max_charge_level(*level).await?;
        }
        Commands::SuggestedMinimumChargeLimit => {
            let proxy = BatteryChargeLimit1Proxy::new(&conn).await?;
            let limit = proxy.suggested_minimum_limit().await?;
            println!("Suggested minimum charge limit: {limit}");
        }
        Commands::ReloadConfig => {
            let proxy = Manager2Proxy::new(&conn).await?;
            proxy.reload_config().await?;
        }
        Commands::GetDeviceModel => {
            let proxy = Manager2Proxy::new(&conn).await?;
            let (device, variant) = proxy.device_model().await?;
            println!("Model: {device}");
            println!("Variant: {variant}");
        }
        Commands::GetScreenReaderEnabled => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let enabled = proxy.enabled().await?;
            println!("Enabled: {enabled}");
        }
        Commands::SetScreenReaderEnabled { enable } => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            proxy.set_enabled(*enable).await?;
        }
        Commands::GetScreenReaderRate => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let rate = proxy.rate().await?;
            println!("Rate: {rate}");
        }
        Commands::SetScreenReaderRate { rate } => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            proxy.set_rate(*rate).await?;
        }
        Commands::GetScreenReaderPitch => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let pitch = proxy.pitch().await?;
            println!("Pitch: {pitch}");
        }
        Commands::SetScreenReaderPitch { pitch } => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            proxy.set_pitch(*pitch).await?;
        }
        Commands::GetScreenReaderVolume => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let volume = proxy.volume().await?;
            println!("Volume: {volume}");
        }
        Commands::SetScreenReaderVolume { volume } => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            proxy.set_volume(*volume).await?;
        }
        Commands::GetScreenReaderMode => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let mode = proxy.mode().await?;
            match ScreenReaderMode::try_from(mode.as_str()) {
                Ok(s) => println!("Screen Reader Mode: {s}"),
                Err(_) => println!("Got unknown screen reader mode value {mode} from backend"),
            }
        }
        Commands::SetScreenReaderMode { mode } => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            proxy.set_mode(mode.to_string().as_str()).await?;
        }
        Commands::TriggerScreenReaderAction { action } => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let timestamp = clock_gettime(ClockId::CLOCK_MONOTONIC_RAW)?;
            let now = timestamp.tv_sec() * 1_000_000_000 + timestamp.tv_nsec();
            proxy
                .trigger_action(action.to_string().as_str(), now.try_into()?)
                .await?;
        }
        Commands::GetScreenReaderVoice => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let voice = proxy.voice().await?;
            println!("Voice: {voice}");
        }
        Commands::SetScreenReaderVoice { voice } => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            proxy.set_voice(voice).await?;
        }
        Commands::GetScreenReaderLocales => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let locales = proxy.voice_locales().await?;
            println!("Locales:\n");
            for locale in locales.into_iter().sorted() {
                println!("- {locale}");
            }
        }
        Commands::GetScreenReaderVoicesForLocale { locale } => {
            let proxy = ScreenReader1Proxy::new(&conn).await?;
            let voice_list = proxy.voices_for_locale().await?;
            let voices = voice_list
                .get(locale)
                .ok_or_else(|| anyhow!("Unable to load voices map"))?;
            println!("Voices:\n");
            for voice in voices.iter().sorted() {
                println!("- {voice}");
            }
        }
        Commands::SwitchToDesktopMode => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            proxy.switch_to_desktop_mode().await?;
        }
        Commands::SwitchToGameMode => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            proxy.switch_to_game_mode().await?;
        }
        Commands::SwitchToLoginMode { mode } => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            proxy
                .switch_to_login_mode(mode.to_string().as_str())
                .await?;
        }
        Commands::SetDefaultDesktopSession { session } => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            proxy.set_default_desktop_session(session.as_str()).await?;
        }
        Commands::GetDefaultDesktopSession => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            let session = proxy.default_desktop_session().await?;
            println!("{session}");
        }
        Commands::SetDefaultLoginMode { mode } => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            proxy
                .set_default_login_mode(mode.to_string().as_str())
                .await?;
        }
        Commands::GetDefaultLoginMode => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            let mode = proxy.default_login_mode().await?;
            println!("{mode}");
        }
        Commands::GetValidDesktopSessions => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            let sessions = proxy.valid_desktop_sessions().await?;
            println!("Sessions:\n");
            for session in sessions.into_iter().sorted() {
                println!("- {session}");
            }
        }
        Commands::CleanTemporarySessions => {
            let proxy = SessionManagement1Proxy::new(&conn).await?;
            proxy.clean_temporary_sessions().await?;
        }
    }

    Ok(())
}
