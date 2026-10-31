//! Unsafe Win32 used by the benchmark harness. Keep every unsafe block here.

use crate::probe::DeviceSnapshot;

pub fn device_snapshot() -> DeviceSnapshot {
    DeviceSnapshot {
        monitors: monitor_count().ok(),
        audio_outputs: audio_output_count().ok(),
    }
}

pub fn power_milliwatts() -> Option<f64> {
    discharge_milliwatts().ok().flatten()
}

pub fn power_note() -> String {
    match discharge_milliwatts() {
        Ok(Some(_)) => "power: Windows SystemBatteryState discharge rate".to_string(),
        Ok(None) => {
            "power: unavailable while on AC or without a discharging battery; no external meter"
                .to_string()
        }
        Err(error) => format!("power: Windows energy estimate failed: {error}"),
    }
}

fn monitor_count() -> Result<u32, String> {
    use windows::core::BOOL;
    use windows::Win32::Foundation::LPARAM;
    use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};

    unsafe extern "system" fn count_monitor(
        _monitor: HMONITOR,
        _dc: HDC,
        _rect: *mut windows::Win32::Foundation::RECT,
        data: LPARAM,
    ) -> BOOL {
        let count = data.0 as *mut u32;
        if !count.is_null() {
            unsafe {
                *count = (*count).saturating_add(1);
            }
        }
        BOOL(1)
    }

    let mut count = 0u32;
    let listed = unsafe {
        EnumDisplayMonitors(
            None,
            None,
            Some(count_monitor),
            LPARAM(&mut count as *mut u32 as isize),
        )
    };
    if !listed.as_bool() && count == 0 {
        return Err("EnumDisplayMonitors failed".to_string());
    }
    Ok(count)
}

fn audio_output_count() -> Result<u32, String> {
    use windows::Win32::Media::Audio::{
        eRender, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED,
    };

    unsafe {
        let _initialized = CoInitializeEx(None, COINIT_MULTITHREADED);
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|error| error.to_string())?;
        let endpoints = enumerator
            .EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE)
            .map_err(|error| error.to_string())?;
        endpoints.GetCount().map_err(|error| error.to_string())
    }
}

fn discharge_milliwatts() -> Result<Option<f64>, String> {
    use std::mem::size_of;

    use windows::Win32::System::Power::{
        CallNtPowerInformation, SystemBatteryState, SYSTEM_BATTERY_STATE,
    };

    let mut state = SYSTEM_BATTERY_STATE::default();
    let status = unsafe {
        CallNtPowerInformation(
            SystemBatteryState,
            None,
            0,
            Some(&mut state as *mut SYSTEM_BATTERY_STATE as *mut _),
            u32::try_from(size_of::<SYSTEM_BATTERY_STATE>())
                .map_err(|_| "battery state does not fit a u32".to_string())?,
        )
    };
    if status.is_err() {
        return Err(format!("CallNtPowerInformation returned {status:?}"));
    }
    if state.AcOnLine || !state.BatteryPresent || !state.Discharging || state.Rate == 0 {
        return Ok(None);
    }
    Ok(Some(f64::from(state.Rate)))
}
