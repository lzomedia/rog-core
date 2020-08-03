use crate::config::Config;
use log::{error, info, warn};
use std::error::Error;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::mpsc::Receiver;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

static FAN_TYPE_1_PATH: &str = "/sys/devices/platform/asus-nb-wmi/throttle_thermal_policy";
static FAN_TYPE_2_PATH: &str = "/sys/devices/platform/asus-nb-wmi/fan_boost_mode";
static AMD_BOOST_PATH: &str = "/sys/devices/system/cpu/cpufreq/boost";

pub struct CtrlFanAndCPU {
    path: &'static str,
}

impl CtrlFanAndCPU {
    pub(super) fn new() -> Result<Self, Box<dyn Error>> {
        let path = CtrlFanAndCPU::get_fan_path()?;

        Ok(CtrlFanAndCPU { path })
    }

    /// Spawns two tasks which continuously check for changes
    pub(crate) fn spawn_task(
        ctrlr: CtrlFanAndCPU,
        config: Arc<Mutex<Config>>,
        mut recv: Receiver<u8>,
    ) -> JoinHandle<()> {
        let gate1 = Arc::new(Mutex::new(ctrlr));
        let gate2 = gate1.clone();
        let config1 = config.clone();
        // spawn an endless loop
        tokio::spawn(async move {
            while let Some(mode) = recv.recv().await {
                let mut config = config1.lock().await;
                if let Ok(mut lock) = gate1.try_lock() {
                    lock.set_fan_mode(mode, &mut config)
                        .unwrap_or_else(|err| warn!("{:?}", err));
                }
            }
        });
        // need to watch file path
        tokio::spawn(async move {
            loop {
                if let Ok(mut lock) = gate2.try_lock() {
                    let mut config = config.lock().await;
                    lock.fan_mode_check_change(&mut config)
                        .unwrap_or_else(|err| warn!("{:?}", err));
                }

                tokio::time::delay_for(std::time::Duration::from_millis(500)).await;
            }
        })
    }

    fn get_fan_path() -> Result<&'static str, std::io::Error> {
        if Path::new(FAN_TYPE_1_PATH).exists() {
            Ok(FAN_TYPE_1_PATH)
        } else if Path::new(FAN_TYPE_2_PATH).exists() {
            Ok(FAN_TYPE_2_PATH)
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Fan mode not available",
            ))
        }
    }

    pub(super) fn fan_mode_reload(&mut self, config: &mut Config) -> Result<(), Box<dyn Error>> {
        let mut file = OpenOptions::new().write(true).open(self.path)?;
        file.write_all(format!("{:?}\n", config.fan_mode).as_bytes())
            .unwrap_or_else(|err| error!("Could not write to {}, {:?}", self.path, err));
        self.set_pstate_for_fan_mode(FanLevel::from(config.fan_mode), config)?;
        info!("Reloaded fan mode: {:?}", FanLevel::from(config.fan_mode));
        Ok(())
    }

    pub(super) fn fan_mode_check_change(
        &mut self,
        config: &mut Config,
    ) -> Result<(), Box<dyn Error>> {
        let mut file = OpenOptions::new().read(true).open(self.path)?;
        let mut buf = [0u8; 1];
        file.read_exact(&mut buf)?;
        if let Some(num) = char::from(buf[0]).to_digit(10) {
            if config.fan_mode != num as u8 {
                config.fan_mode = num as u8;
                config.write();
                self.set_pstate_for_fan_mode(FanLevel::from(config.fan_mode), config)?;
                info!(
                    "Fan mode was changed: {:?}",
                    FanLevel::from(config.fan_mode)
                );
            }
            return Ok(());
        }
        let err = std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Fan-level could not be parsed",
        );
        Err(Box::new(err))
    }

    pub(super) fn set_fan_mode(
        &mut self,
        n: u8,
        config: &mut Config,
    ) -> Result<(), Box<dyn Error>> {
        let mut fan_ctrl = OpenOptions::new().write(true).open(self.path)?;

        config.fan_mode = n;
        config.write();
        fan_ctrl
            .write_all(format!("{:?}\n", config.fan_mode).as_bytes())
            .unwrap_or_else(|err| error!("Could not write to {}, {:?}", self.path, err));
        info!("Fan mode set to: {:?}", FanLevel::from(config.fan_mode));
        self.set_pstate_for_fan_mode(FanLevel::from(n), config)?;
        Ok(())
    }

    fn set_pstate_for_fan_mode(
        &self,
        mode: FanLevel,
        config: &mut Config,
    ) -> Result<(), Box<dyn Error>> {
        // Set CPU pstate
        if let Ok(pstate) = intel_pstate::PState::new() {
            match mode {
                FanLevel::Normal => {
                    pstate.set_min_perf_pct(config.mode_performance.normal.min_percentage)?;
                    pstate.set_max_perf_pct(config.mode_performance.normal.max_percentage)?;
                    pstate.set_no_turbo(config.mode_performance.normal.no_turbo)?;
                    info!(
                        "Intel CPU Power: min: {:?}%, max: {:?}%, turbo: {:?}",
                        config.mode_performance.normal.min_percentage,
                        config.mode_performance.normal.max_percentage,
                        !config.mode_performance.normal.no_turbo
                    );
                }
                FanLevel::Boost => {
                    pstate.set_min_perf_pct(config.mode_performance.boost.min_percentage)?;
                    pstate.set_max_perf_pct(config.mode_performance.boost.max_percentage)?;
                    pstate.set_no_turbo(config.mode_performance.boost.no_turbo)?;
                    info!(
                        "Intel CPU Power: min: {:?}%, max: {:?}%, turbo: {:?}",
                        config.mode_performance.boost.min_percentage,
                        config.mode_performance.boost.max_percentage,
                        !config.mode_performance.boost.no_turbo
                    );
                }
                FanLevel::Silent => {
                    pstate.set_min_perf_pct(config.mode_performance.silent.min_percentage)?;
                    pstate.set_max_perf_pct(config.mode_performance.silent.max_percentage)?;
                    pstate.set_no_turbo(config.mode_performance.silent.no_turbo)?;
                    info!(
                        "Intel CPU Power: min: {:?}%, max: {:?}%, turbo: {:?}",
                        config.mode_performance.silent.min_percentage,
                        config.mode_performance.silent.max_percentage,
                        !config.mode_performance.silent.no_turbo
                    );
                }
            }
        } else {
            info!("Setting pstate for AMD CPU");
            // must be AMD CPU
            let mut file = OpenOptions::new()
                .write(true)
                .open(AMD_BOOST_PATH)
                .map_err(|err| {
                    warn!("Failed to open AMD boost: {:?}", err);
                    err
                })?;
            match mode {
                FanLevel::Normal => {
                    let boost = if config.mode_performance.normal.no_turbo {
                        "0"
                    } else {
                        "1"
                    }; // opposite of Intel
                    file.write_all(boost.as_bytes()).unwrap_or_else(|err| {
                        error!("Could not write to {}, {:?}", AMD_BOOST_PATH, err)
                    });
                    info!("AMD CPU Turbo: {:?}", boost);
                }
                FanLevel::Boost => {
                    let boost = if config.mode_performance.boost.no_turbo {
                        "0"
                    } else {
                        "1"
                    };
                    file.write_all(boost.as_bytes()).unwrap_or_else(|err| {
                        error!("Could not write to {}, {:?}", AMD_BOOST_PATH, err)
                    });
                    info!("AMD CPU Turbo: {:?}", boost);
                }
                FanLevel::Silent => {
                    let boost = if config.mode_performance.silent.no_turbo {
                        "0"
                    } else {
                        "1"
                    };
                    file.write_all(boost.as_bytes()).unwrap_or_else(|err| {
                        error!("Could not write to {}, {:?}", AMD_BOOST_PATH, err)
                    });
                    info!("AMD CPU Turbo: {:?}", boost);
                }
            }
        }
        Ok(())
    }
}

use crate::error::RogError;

#[derive(Debug)]
pub enum FanLevel {
    Normal,
    Boost,
    Silent,
}

impl FromStr for FanLevel {
    type Err = RogError;

    fn from_str(s: &str) -> Result<Self, RogError> {
        match s.to_lowercase().as_str() {
            "normal" => Ok(FanLevel::Normal),
            "boost" => Ok(FanLevel::Boost),
            "silent" => Ok(FanLevel::Silent),
            _ => Err(RogError::ParseFanLevel),
        }
    }
}

impl From<u8> for FanLevel {
    fn from(n: u8) -> Self {
        match n {
            0 => FanLevel::Normal,
            1 => FanLevel::Boost,
            2 => FanLevel::Silent,
            _ => FanLevel::Normal,
        }
    }
}

impl From<FanLevel> for u8 {
    fn from(n: FanLevel) -> Self {
        match n {
            FanLevel::Normal => 0,
            FanLevel::Boost => 1,
            FanLevel::Silent => 2,
        }
    }
}
