# VALORANT settings keys (confirmed on the owner's PC, 2026-10-12)

Captured with a read-only PowerShell filter that printed key names only (no values), from the most
recently written files under `%LOCALAPPDATA%\VALORANT\Saved\Config` (one folder per Riot account; the
owner has several, so pick the newest file by last-write time).

## GameUserSettings.ini (standard Unreal keys)

- Display: `ResolutionSizeX`, `ResolutionSizeY`, `LastUserConfirmedResolutionSizeX`,
  `LastUserConfirmedResolutionSizeY`, `FullscreenMode`, `PreferredFullscreenMode`,
  `LastConfirmedFullscreenMode`, `bUseDynamicResolution`
- Frame pacing: `bUseVSync`, `FrameRateLimit`
- Quality: `sg.AntiAliasingQuality`, `sg.EffectsQuality`, `sg.FoliageQuality`,
  `sg.GlobalIlluminationQuality`, `sg.PostProcessQuality`, `sg.ReflectionQuality`, `sg.ShadingQuality`,
  `sg.ShadowQuality`, `sg.TextureQuality`, `sg.ViewDistanceQuality`
- Audio (not needed): `AudioQualityLevel`, `LastConfirmedAudioQualityLevel`

## RiotUserSettings.ini (VALORANT's own keys, `EAres<Type>SettingName::<Name>=value`)

- `EAresBoolSettingName::MultithreadedRenderingEnabled`
- `EAresBoolSettingName::ShadowsEnabled`
- `EAresIntSettingName::AnisotropicFiltering`
- `EAresIntSettingName::AntiAliasing`
- `EAresIntSettingName::DetailQuality`
- `EAresIntSettingName::MaterialQuality`
- `EAresIntSettingName::NvidiaReflexLowLatencySetting`
- `EAresIntSettingName::PlayerPerfShowFrameRate`
- `EAresIntSettingName::TextureQuality`
- `EAresIntSettingName::UIQuality`
- (`EAresStringSettingName::SavedCrosshairProfileData` matched the filter incidentally; not needed.)

## Notes for the settings reader

- No frame-rate-limit keys (in-menu, background, on-battery, always) appeared in RiotUserSettings.ini.
  The owner plays uncapped (decision 35), so keys left at their default seem not to be written: treat
  an absent key as "default", never as an error. Value encodings (e.g. Reflex 0/1/2) still need
  confirming from a diagnostics run.
- Read-only, always (decision 34): VALORANT syncs these settings with Riot's servers.
