use std::io::{self, IsTerminal, Write};

use anyhow::Result;

use crate::{
    alert,
    config::Config,
    paths::{display_path, StatePaths},
};

use super::{save_selected_keys, DiscoveredSshKey, GREEN, RESET};

pub(super) fn configure_line_mode(
    discovered: Vec<DiscoveredSshKey>,
    mut selected: Vec<bool>,
    mut config: Config,
) -> Result<i32> {
    loop {
        print_selector(&discovered, &selected, &config)?;
        print!("Command [number toggles, s save, q quit]: ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        let input = input.trim();

        if input.eq_ignore_ascii_case("q") {
            println!("No SSH configuration changes saved.");
            return Ok(0);
        }

        if input.eq_ignore_ascii_case("s") || input.is_empty() {
            save_selected_keys(&mut config, &discovered, &selected)?;
            println!(
                "Saved {} selected SSH key(s) to {}",
                config.ssh.selected_keys.len(),
                display_path(&StatePaths::default()?.config_yaml)
            );
            return Ok(0);
        }

        for token in input
            .split(|ch: char| ch == ',' || ch.is_whitespace())
            .filter(|token| !token.is_empty())
        {
            match token.parse::<usize>() {
                Ok(index) if index >= 1 && index <= selected.len() => {
                    let slot = &mut selected[index - 1];
                    *slot = !*slot;
                }
                _ => println!("Ignoring invalid selection: {token}"),
            }
        }
    }
}

fn print_selector(keys: &[DiscoveredSshKey], selected: &[bool], config: &Config) -> Result<()> {
    let colors_enabled = alert::colors_enabled_for_config(config, io::stdout().is_terminal());
    println!();
    println!("Detected SSH keys:");
    for (index, (key, is_selected)) in keys.iter().zip(selected.iter()).enumerate() {
        let marker = if *is_selected { "[✓]" } else { "[ ]" };
        let first_line = format!("{marker} {}. {}", index + 1, key.display_path);
        if *is_selected && colors_enabled {
            println!("{GREEN}{first_line}{RESET}");
        } else {
            println!("{first_line}");
        }

        let detail = format!(
            "    {}{}{}{}{}",
            key.key_type.as_deref().unwrap_or("unknown"),
            key.fingerprint
                .as_deref()
                .map(|fp| format!(" {fp}"))
                .unwrap_or_default(),
            key.comment
                .as_deref()
                .map(|comment| format!(" {comment}"))
                .unwrap_or_default(),
            if key.has_public_pair { " [pub]" } else { "" },
            match key.permissions_ok {
                Some(true) => "",
                Some(false) => " broad permissions",
                None => "",
            }
        );
        if *is_selected && colors_enabled {
            println!("{GREEN}{detail}{RESET}");
        } else {
            println!("{detail}");
        }
    }
    println!();
    if colors_enabled {
        println!("Selected rows are green. Unselected rows use the default terminal color.");
    } else {
        println!("Selected rows use [✓]. Unselected rows use [ ].");
    }
    Ok(())
}
