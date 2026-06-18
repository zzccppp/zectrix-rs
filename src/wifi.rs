use esp_hal::peripherals::WIFI;
use esp_radio::wifi::{self, scan::ScanConfig};
use log::{info, warn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WifiScanStatus {
    Pending,
    Ready,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WifiSnapshot {
    pub status: WifiScanStatus,
    pub ap_count: u8,
    pub best_rssi: i8,
    pub best_channel: u8,
}

impl WifiSnapshot {
    pub const PENDING: Self = Self {
        status: WifiScanStatus::Pending,
        ap_count: 0,
        best_rssi: 0,
        best_channel: 0,
    };

    pub fn status_text(self) -> &'static str {
        match self.status {
            WifiScanStatus::Pending => "扫描中",
            WifiScanStatus::Ready if self.ap_count == 0 => "未发现",
            WifiScanStatus::Ready => "已扫描",
            WifiScanStatus::Failed => "扫描失败",
        }
    }
}

pub async fn scan_once(wifi_peripheral: WIFI<'_>) -> WifiSnapshot {
    info!("WiFi scan: initializing controller");
    let (mut controller, _interfaces) = match wifi::new(wifi_peripheral, Default::default()) {
        Ok(parts) => parts,
        Err(err) => {
            warn!("WiFi scan: controller init failed: {:?}", err);
            return WifiSnapshot {
                status: WifiScanStatus::Failed,
                ..WifiSnapshot::PENDING
            };
        }
    };

    let scan_config = ScanConfig::default().with_max(12);
    info!("WiFi scan: start");
    let access_points = match controller.scan_async(&scan_config).await {
        Ok(access_points) => access_points,
        Err(err) => {
            warn!("WiFi scan: failed: {:?}", err);
            return WifiSnapshot {
                status: WifiScanStatus::Failed,
                ..WifiSnapshot::PENDING
            };
        }
    };

    let mut snapshot = WifiSnapshot {
        status: WifiScanStatus::Ready,
        ap_count: access_points.len().min(u8::MAX as usize) as u8,
        best_rssi: 0,
        best_channel: 0,
    };

    if let Some(best) = access_points.iter().max_by_key(|ap| ap.signal_strength) {
        snapshot.best_rssi = best.signal_strength;
        snapshot.best_channel = best.channel;
        info!(
            "WiFi scan: best ssid=\"{}\" rssi={}dBm channel={}",
            best.ssid.as_str(),
            best.signal_strength,
            best.channel,
        );
    }

    for (index, ap) in access_points.iter().take(5).enumerate() {
        info!(
            "WiFi AP[{}]: ssid=\"{}\" rssi={}dBm channel={} auth={:?}",
            index,
            ap.ssid.as_str(),
            ap.signal_strength,
            ap.channel,
            ap.auth_method,
        );
    }
    info!("WiFi scan: complete, ap_count={}", snapshot.ap_count);

    snapshot
}
