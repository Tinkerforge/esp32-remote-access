use semver::Version;

/// Minimums are one patch above the latest releases as of 2026-11-16,
/// from esp32-firmware/software/changelog_*.txt (released 2026-10-27).
/// Keep these fixed so later releases do not disable IPv6 for supported devices.
pub(crate) fn supports_ipv6_tunnel(device_type: Option<&str>, firmware_version: &str) -> bool {
    // Management stores the complete firmware User-Agent, including its version.
    let product = device_type
        .unwrap_or_default()
        .split('/')
        .next()
        .unwrap_or_default();
    let minimum = match product {
        "Tinkerforge-WARP_Charger" => Version::new(2, 13, 6),
        "Tinkerforge-WARP2_Charger"
        | "Tinkerforge-WARP3_Charger"
        | "Tinkerforge-WARP4_Charger"
        | "ELTAKO-ELTAKO_Wallbox" => Version::new(2, 13, 7),
        "Tinkerforge-WARP_Energy_Manager" => Version::new(2, 9, 7),
        "Tinkerforge-WARP_Energy_Manager_2.0" | "effizient2-Smart_Energy_Broker" => {
            Version::new(1, 8, 7)
        }
        _ => return false,
    };
    Version::parse(firmware_version).is_ok_and(|version| version >= minimum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_requires_a_release_above_current_latest_for_each_product() {
        for (product, latest, minimum, newer) in [
            ("Tinkerforge-WARP_Charger", "2.13.5", "2.13.6", "2.14.0"),
            ("Tinkerforge-WARP2_Charger", "2.13.6", "2.13.7", "2.14.0"),
            ("Tinkerforge-WARP3_Charger", "2.13.6", "2.13.7", "2.14.0"),
            ("Tinkerforge-WARP4_Charger", "2.13.6", "2.13.7", "3.0.0"),
            ("ELTAKO-ELTAKO_Wallbox", "2.13.6", "2.13.7", "2.14.0"),
            (
                "Tinkerforge-WARP_Energy_Manager",
                "2.9.6",
                "2.9.7",
                "2.10.0",
            ),
            (
                "Tinkerforge-WARP_Energy_Manager_2.0",
                "1.8.6",
                "1.8.7",
                "1.9.0",
            ),
            ("effizient2-Smart_Energy_Broker", "1.8.6", "1.8.7", "2.0.0"),
        ] {
            let user_agent = format!("{product}/{minimum}+1234abcd");
            let device_type = Some(user_agent.as_str());
            assert!(!supports_ipv6_tunnel(device_type, latest), "{product}");
            assert!(supports_ipv6_tunnel(device_type, minimum), "{product}");
            assert!(supports_ipv6_tunnel(device_type, newer), "{product}");
            assert!(supports_ipv6_tunnel(
                device_type,
                &format!("{minimum}+1234abcd")
            ));
            assert!(!supports_ipv6_tunnel(
                device_type,
                &format!("{minimum}-beta.1")
            ));
            for invalid in ["", "invalid", "2.13", "v2.13.5"] {
                assert!(!supports_ipv6_tunnel(device_type, invalid));
            }
        }
    }

    #[test]
    fn unknown_products_do_not_enable_ipv6() {
        for device_type in [
            None,
            Some(""),
            Some("unknown"),
            Some("ESP32 HTTP Client/1.0"),
        ] {
            assert!(!supports_ipv6_tunnel(device_type, "99.0.0"));
        }
    }
}
