use std::collections::HashMap;

use chrono::Utc;
use primitives::{sentry::Event, Address, Campaign, Channel, UnifiedNum};

use crate::Session;

pub enum OperatingSystem {
    Linux,
    Whitelisted(String),
    Other,
}

impl OperatingSystem {
    const LINUX_DISTROS: [&'static str; 17] = [
        "Arch",
        "CentOS",
        "Slackware",
        "Fedora",
        "Debian",
        "Deepin",
        "elementary OS",
        "Gentoo",
        "Mandriva",
        "Manjaro",
        "Mint",
        "PCLinuxOS",
        "Raspbian",
        "Sabayon",
        "SUSE",
        "Ubuntu",
        "RedHat",
    ];
    const WHITELISTED: [&'static str; 18] = [
        "Android",
        "Android-x86",
        "iOS",
        "BlackBerry",
        "Chromium OS",
        "Fuchsia",
        "Mac OS",
        "Windows",
        "Windows Phone",
        "Windows Mobile",
        "Linux",
        "NetBSD",
        "Nintendo",
        "OpenBSD",
        "PlayStation",
        "Tizen",
        "Symbian",
        "KAIOS",
    ];
}

fn mapOS(os_name: &str) -> OperatingSystem {
    if let Some(_) = OperatingSystem::LINUX_DISTROS
        .iter()
        .find(|distro| os_name.eq(**distro))
    {
        OperatingSystem::Linux
    } else if let Some(_) = OperatingSystem::WHITELISTED
        .iter()
        .find(|whitelisted| os_name.eq(**whitelisted))
    {
        OperatingSystem::Whitelisted(os_name.into())
    } else {
        OperatingSystem::Other
    }
}

fn record(
    campaign: Campaign,
    session: Session,
    events_with_payouts: Vec<(Event, HashMap<Address, UnifiedNum>)>,
) -> Result<(), ()> {
    let os = mapOS(&session.os.unwrap_or_default());
    let time = Utc::now();

    for (event, event_payout) in events_with_payouts {
        let (publisher, ad_unit) = {
            let (publisher, event_ad_unit) = match event {
                Event::Impression {
                    publisher, ad_unit, ..
                } => (publisher, ad_unit),
                Event::Click {
                    publisher, ad_unit, ..
                } => (publisher, ad_unit),
            };

            let ad_unit = event_ad_unit.and_then(|ipfs| {
                campaign
                    .ad_units
                    .iter()
                    .find(|ad_unit| ad_unit.ipfs == ipfs)
            });
            (publisher, ad_unit)
        };
        
        // DB: Insert or Update all events
    }

    Ok(())
}
