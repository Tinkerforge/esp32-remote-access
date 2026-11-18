use semver::Version;

// 2026-11-18 13:14:04 UTC. Keep this fixed so supported builds stay eligible.
const IPV6_BUILD_TIMESTAMP_CUTOFF: u32 = 1_795_007_644;

/// The baseline versions are from esp32-firmware/software/changelog_*.txt
/// (released 2026-10-27). Firmware reports its Unix build time as hex metadata.
pub(crate) fn supports_ipv6_tunnel(device_type: Option<&str>, firmware_version: &str) -> bool {
    // Management stores the complete firmware User-Agent, including its version.
    let product = device_type
        .unwrap_or_default()
        .split('/')
        .next()
        .unwrap_or_default();
    let release = match product {
        "Tinkerforge-WARP_Charger" => Version::new(2, 13, 5),
        "Tinkerforge-WARP2_Charger"
        | "Tinkerforge-WARP3_Charger"
        | "Tinkerforge-WARP4_Charger"
        | "ELTAKO-ELTAKO_Wallbox" => Version::new(2, 13, 6),
        "Tinkerforge-WARP_Energy_Manager" => Version::new(2, 9, 6),
        "Tinkerforge-WARP_Energy_Manager_2.0" | "effizient2-Smart_Energy_Broker" => {
            Version::new(1, 8, 6)
        }
        _ => return false,
    };
    let Ok(version) = Version::parse(firmware_version) else {
        return false;
    };

    if version.cmp_precedence(&release).is_eq() {
        u32::from_str_radix(version.build.as_str(), 16)
            .is_ok_and(|timestamp| timestamp > IPV6_BUILD_TIMESTAMP_CUTOFF)
    } else {
        version >= Version::new(release.major, release.minor, release.patch + 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipv6_supports_builds_of_the_current_release_after_the_cutoff() {
        for (product, release, older, next) in [
            ("Tinkerforge-WARP_Charger", "2.13.5", "2.13.4", "2.13.6"),
            ("Tinkerforge-WARP2_Charger", "2.13.6", "2.13.5", "2.13.7"),
            ("Tinkerforge-WARP3_Charger", "2.13.6", "2.13.5", "2.13.7"),
            ("Tinkerforge-WARP4_Charger", "2.13.6", "2.13.5", "2.13.7"),
            ("ELTAKO-ELTAKO_Wallbox", "2.13.6", "2.13.5", "2.13.7"),
            ("Tinkerforge-WARP_Energy_Manager", "2.9.6", "2.9.5", "2.9.7"),
            (
                "Tinkerforge-WARP_Energy_Manager_2.0",
                "1.8.6",
                "1.8.5",
                "1.8.7",
            ),
            ("effizient2-Smart_Energy_Broker", "1.8.6", "1.8.5", "1.8.7"),
        ] {
            let user_agent = format!("{product}/{release}+6afda49d");
            let device_type = Some(user_agent.as_str());
            for (version, expected) in [
                (format!("{older}+6afda49d"), false),
                (release.to_owned(), false),
                (format!("{release}+6afda49b"), false),
                (format!("{release}+6afda49c"), false),
                (format!("{release}+6afda49d"), true),
                (format!("{release}+6AFDA49D"), true),
                (format!("{release}+other"), false),
                (format!("{release}+100000000"), false),
                (format!("{release}-beta.1+6afda49d"), false),
                (format!("{next}-beta.1+6afda49d"), false),
                (next.to_owned(), true),
                (format!("{next}+1234abcd"), true),
                ("99.0.0".to_owned(), true),
                ("invalid".to_owned(), false),
            ] {
                assert_eq!(
                    supports_ipv6_tunnel(device_type, &version),
                    expected,
                    "{product}/{version}"
                );
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
