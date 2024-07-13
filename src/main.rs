#![feature(duration_constructors)]

// /sys/class/net/eth0/statistics/tx_packets

use std::{fs, path::PathBuf, str::FromStr, sync::{atomic::AtomicU64, Arc, Mutex}, thread, time::{Duration, SystemTime, UNIX_EPOCH}};
use num_bigint::{BigInt, BigUint, ToBigInt, ToBigUint};

use serde_derive::{Deserialize, Serialize};

fn main() {
    let meta = Arc::new(Mutex::new(Meta::load()));
    let config = Config::load();
    let save_ms = config.save_interval_ms;
    let meta2 = meta.clone();
    // interval saver thread
    thread::spawn(move || {
        let meta = meta2;
        loop {
            thread::sleep(Duration::from_millis(save_ms));
            let mut meta = meta.lock().unwrap();
            meta.last_saved_bytes = fetch_outbound_bytes().to_string();
            meta.store();
        }
    });
    let meta2 = meta.clone();
    let reset_delay = config.capture_timeframe_ms;
    let max_bytes = config.max_bytes;
    // limit resetter thread
    thread::spawn(move || {
        let meta = meta2;
        loop {
            let c_meta = meta.lock().unwrap();
            let sleep_ms = c_meta.reset_at_ms.saturating_sub(current_time_millis() as u64);
            drop(c_meta);
            thread::sleep(Duration::from_millis(sleep_ms));
            let mut meta = meta.lock().unwrap();
            let dist = BigUint::from_str(&meta.last_saved_bytes).unwrap() - BigUint::from_str(&meta.starting_bytes).unwrap();
            if dist > max_bytes.to_biguint().unwrap() {
                disable_lowered_bandwidth();
            }
            meta.starting_bytes = fetch_outbound_bytes().to_string();
            meta.reset_at_ms = current_time_millis() as u64 + reset_delay;
            meta.store();
        }
    });
    // byte amount saver
    loop {
        thread::sleep(Duration::from_millis(config.check_interval_ms));
        let curr_bytes = fetch_outbound_bytes();
        let meta = meta.lock().unwrap();
        let dist = curr_bytes.clone() - BigUint::from_str(&meta.starting_bytes).unwrap();
        if dist >= config.save_every_n_bytes.to_biguint().unwrap() {
            Meta {
                reset_at_ms: meta.reset_at_ms,
                starting_bytes: meta.starting_bytes.clone(),
                last_saved_bytes: curr_bytes.to_string(),
            }.store();
        }
        if dist > config.max_bytes.to_biguint().unwrap() {
            enable_lower_bandwidth(config.lower_limit_bytes);
            let sleep_time = meta.reset_at_ms.saturating_sub(current_time_millis() as u64);
            drop(meta);
            thread::sleep(Duration::from_millis(sleep_time));
        }
    }
}

fn enable_lower_bandwidth(limit: u64) {

}

fn disable_lowered_bandwidth() {

}

fn fetch_outbound_bytes() -> BigUint {
    let mut sum = BigUint::ZERO;
    for interface in fs::read_dir("/sys/class/net").unwrap() {
        if let Ok(interface) = interface {
            let mut path = interface.path();
            path.push("statistics");
            path.push("tx_bytes");
            sum += BigUint::from_str(fs::read_to_string(path).unwrap().as_str()).unwrap();
        }
    }
    sum
}

#[derive(Serialize, Deserialize)]
struct Config {
    save_interval_ms: u64,
    check_interval_ms: u64,
    save_every_n_bytes: u64,
    capture_timeframe_ms: u64,
    max_bytes: u64,
    lower_limit_bytes: u64,
}

impl Config {

    fn load() -> Self {
        let cfg_path = dirs::config_dir().map(|mut dir| {
            dir.push("traffic_tracker");
            dir.push("config.json");
            dir
        }).unwrap_or_else(|| PathBuf::from_str("./traffic_tracker/config.json").unwrap());
        if !cfg_path.exists() {
            let cfg = Config {
                save_interval_ms: 1000 * 60,
                check_interval_ms: 1000 * 10,
                save_every_n_bytes: 1024 * 1024 * 64,
                capture_timeframe_ms: 1000 * 60 * 60 * 24 * 7,
                max_bytes: 1024 * 1024 * 1024 * 1024,
                lower_limit_bytes: 64 * 1024,
            };
            fs::write(cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
            return cfg;
        }
        serde_json::from_slice(&fs::read(cfg_path).unwrap()).unwrap()
    }

}

#[derive(Serialize, Deserialize)]
struct Meta {
    reset_at_ms: u64,
    starting_bytes: String,
    last_saved_bytes: String,
}

impl Meta {

    fn path() -> PathBuf {
        dirs::config_dir().map(|mut dir| {
            dir.push("traffic_tracker");
            dir.push("meta.json");
            dir
        }).unwrap_or_else(|| PathBuf::from_str("./traffic_tracker/meta.json").unwrap())
    }

    fn load() -> Self {
        let cfg_path = Self::path();
        if !cfg_path.exists() {
            let sent_bytes = fetch_outbound_bytes().to_string();
            let cfg = Meta {
                reset_at_ms: Duration::from_days(7).as_millis() as u64,
                last_saved_bytes: sent_bytes.clone(),
                starting_bytes: sent_bytes,
            };
            fs::write(cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
            return cfg;
        }
        serde_json::from_slice(&fs::read(cfg_path).unwrap()).unwrap()
    }

    fn store(&self) {
        let cfg_path = Self::path();
        fs::write(cfg_path, serde_json::to_string_pretty(self).unwrap()).unwrap();
    }

}

fn current_time_millis() -> u128 {
    let now = SystemTime::now();
    let duration_since_epoch = now.duration_since(UNIX_EPOCH).unwrap();
    let milliseconds = duration_since_epoch.as_millis();
    milliseconds
}
