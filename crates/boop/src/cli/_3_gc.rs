//! GC command and lane-list expiry use the same coordinator activity clock.
use anyhow::Result;
use boop::{bus, gc};
use std::{path::Path, time::SystemTime};

pub(crate) fn run(mail_arg: Option<&Path>, apply: bool) -> Result<()> {
    let mail = super::mail_dir(mail_arg)?;
    let root = boop::trail::lane_target_root()?;
    let trails = boop::trail::lanes_root()?;
    let routes = bus::read_routes(&mail)?;
    let activity = gc::activity(&mail, &routes)?;
    let protected = gc::protected(&mail, &routes, None)?;
    let now = SystemTime::now();
    let candidates = gc::exclude_store(
        &mail,
        gc::collect(&root, &trails, &routes, &protected, &activity, now)?,
    )?;
    println!("path\tbytes\towner lane\tlane state\treason");
    for candidate in &candidates {
        println!(
            "{}\t{}\t{}\t{}\t{}",
            candidate.path.display(),
            candidate.bytes,
            candidate.lane,
            candidate.state,
            candidate.reason
        );
    }
    let expired: Vec<_> = routes
        .iter()
        .filter(|(name, route)| {
            gc::expired_coordinator(
                route,
                protected.contains(*name),
                activity.get(*name).copied(),
                now,
            )
        })
        .map(|(name, route)| (name.clone(), bus::route_to_value(route)))
        .collect();
    for (name, _) in &expired {
        println!(
            "{}#{}\t0\t{}\tdead\tcoordinator dead 7d",
            bus::db_path(&mail)?.display(),
            name,
            name
        );
    }
    if apply {
        // Recollect between mutations so a revived lane or changed route is protected.
        for candidate in candidates {
            let current = bus::read_routes(&mail)?;
            let active = gc::protected(&mail, &current, None)?;
            let clock = gc::activity(&mail, &current)?;
            let fresh = gc::exclude_store(
                &mail,
                gc::collect(&root, &trails, &current, &active, &clock, SystemTime::now())?,
            )?;
            if let Some(candidate) = fresh
                .iter()
                .find(|fresh| fresh.path == candidate.path && fresh.kind == candidate.kind)
            {
                gc::remove(candidate, &root, &trails)?;
            }
        }
        let current = bus::read_routes(&mail)?;
        let active = gc::protected(&mail, &current, None)?;
        let clock = gc::activity(&mail, &current)?;
        bus::cas_update_json(&mail.join("registry.json"), |rows| {
            for (name, expected) in &expired {
                if rows.get(name) == Some(expected)
                    && current.get(name).is_some_and(|route| {
                        gc::expired_coordinator(
                            route,
                            active.contains(name),
                            clock.get(name).copied(),
                            SystemTime::now(),
                        )
                    })
                {
                    rows.remove(name);
                }
            }
            Ok(())
        })?;
    }
    Ok(())
}
