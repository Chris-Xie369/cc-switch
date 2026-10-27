#!/usr/bin/env python3

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]

DISPLAY_KEYS = (
    "claudeDesktopDisplay",
    "claudeDesktopDisplayHint",
    "claudeDesktopDisplayEnable",
    "claudeDesktopDisplayName",
    "claudeDesktopDisplaySubtitle",
    "claudeDesktopDisplayAttribution",
)


def read(path: str) -> str:
    file = ROOT / path
    if not file.is_file():
        raise SystemExit(f"missing {path}")
    return file.read_text(encoding="utf-8")


def need(path: str, needle: str, label: str) -> None:
    text = read(path)
    if needle not in text:
        raise SystemExit(f"missing {label}: {needle!r} in {path}")


def forbid_file(path: str, label: str) -> None:
    if (ROOT / path).exists():
        raise SystemExit(f"retired patch restored: {label} ({path})")


def forbid_text(path: str, needle: str, label: str) -> None:
    if needle in read(path):
        raise SystemExit(f"retired or forbidden marker {label}: {needle!r} in {path}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--npm", required=True, help="package.json version, no -local")
    parser.add_argument(
        "--rust",
        required=True,
        help="Cargo.toml and tauri.conf.json version, with -local",
    )
    args = parser.parse_args()
    if args.npm.endswith("-local") or "-local" not in args.rust:
        raise SystemExit("npm version has no -local; rust version keeps -local")

    need(
        "src-tauri/src/claude_desktop_config.rs",
        "fn inject_display_settings(",
        "B",
    )
    need(
        "src-tauri/src/claude_desktop_config.rs",
        'profile["endUserAttribution"]',
        "B",
    )
    need(
        "src-tauri/src/claude_desktop_config.rs",
        "fn has_non_anthropic_vendor_token(",
        "vendor-word guard",
    )
    need("src-tauri/src/settings.rs", "struct ClaudeDesktopDisplaySettings", "B")
    for locale in ("src/i18n/locales/en.json", "src/i18n/locales/zh.json"):
        for key in DISPLAY_KEYS:
            need(locale, f'"{key}"', "B")

    package = json.loads(read("package.json"))
    if package.get("version") != args.npm:
        raise SystemExit(
            f"package.json version {package.get('version')!r} != {args.npm!r}"
        )
    cargo = read("src-tauri/Cargo.toml")
    if f'version = "{args.rust}"' not in cargo.split("[dependencies]", 1)[0]:
        raise SystemExit(f"Cargo.toml package version is not {args.rust}")
    if f'"version": "{args.rust}"' not in read("src-tauri/tauri.conf.json"):
        raise SystemExit(f"tauri.conf.json version is not {args.rust}")
    lock = read("src-tauri/Cargo.lock")
    name_at = lock.find('name = "cc-switch"')
    if name_at < 0 or f'version = "{args.rust}"' not in lock[name_at : name_at + 200]:
        raise SystemExit(f"Cargo.lock cc-switch version is not {args.rust}")
    forbid_text("src-tauri/tauri.conf.json", "plugins.updater", "C")
    forbid_text("src-tauri/tauri.conf.json", "createUpdaterArtifacts", "C")
    forbid_text("src-tauri/tauri.conf.json", '"updater"', "C")

    need("src-tauri/src/commands/upstream.rs", "fn check_upstream_status(", "D")
    need("src/lib/updater.ts", "check_upstream_status", "D")
    need("src-tauri/src/lib.rs", "commands::upstream::check_upstream_status", "D")

    need("src-tauri/src/dsh_config.rs", "fn sync_dsh_aggregate_now(", "DSH")
    need("src-tauri/src/commands/dsh.rs", "fn sync_dsh_aggregate(", "DSH")
    need("src-tauri/src/commands/mod.rs", "mod dsh;", "DSH")
    if not (ROOT / "src/components/providers/forms/DshProviderForm.tsx").is_file():
        raise SystemExit("missing DSH DshProviderForm.tsx")
    if not (ROOT / "src/config/dshProviderPresets.ts").is_file():
        raise SystemExit("missing DSH dshProviderPresets.ts")
    need("src/components/providers/forms/ProviderForm.tsx", "DshProviderForm", "DSH")
    for locale in ("src/i18n/locales/en.json", "src/i18n/locales/zh.json"):
        need(locale, '"deepseekHarness"', "DSH")

    need(
        "src-tauri/src/proxy/handlers.rs",
        "fn gateway_token_from_headers(",
        "x-api-key",
    )
    need("src-tauri/src/proxy/handlers.rs", '"x-api-key"', "x-api-key")

    forbid_file("src-tauri/src/aggregate.rs", "E")
    for path in (
        "src-tauri/src/lib.rs",
        "src-tauri/src/commands/mod.rs",
    ):
        forbid_text(path, "mod aggregate;", "E")
    need(
        "src-tauri/src/provider.rs",
        'value.get("aggregateRoutes").is_none()',
        "E stays omitted",
    )
    need(
        "src-tauri/src/claude_desktop_config.rs",
        "fn gateway_profile_patch(",
        "A stays replaced",
    )

    markers = subprocess.run(
        ["git", "grep", "-l", "-e", "^<<<<<<< ", "--", "."],
        cwd=ROOT,
        stdout=subprocess.PIPE,
        stderr=subprocess.DEVNULL,
        text=True,
    )
    found = [line for line in markers.stdout.splitlines() if line]
    if found:
        raise SystemExit("conflict markers remain:\n" + "\n".join(found[:30]))

    print(f"preserve ok npm={args.npm} rust={args.rust}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
