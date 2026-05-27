/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useEffect, useRef, useState } from "react";
import {
  Box,
  Button,
  FormControl,
  FormControlLabel,
  MenuItem,
  Paper,
  Select,
  Stack,
  Switch,
  Tab,
  Tabs,
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
} from "@mui/material";
import {
  BrightnessAuto as AutoIcon,
  DarkMode as DarkIcon,
  Extension as IntegrationIcon,
  LightMode as LightIcon,
  Palette as AppearanceIcon,
  Tune as ScanIcon,
  Numbers as FormatIcon,
  Update as UpdateIcon,
} from "@mui/icons-material";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import * as Protocol from "../lib/protocol";
import {
  isBetterMediaInfoFound,
  isMkvtoolnixFound,
  isMpcHcFound,
  setConfig as saveConfig,
} from "../lib/service";
import { useAppStore } from "../lib/store";
import { changeLanguage } from "../i18n";

enum ConfigTab {
  Appearance = "Appearance",
  Scan = "Scan",
  Formatting = "Formatting",
  Integration = "Integration",
  Update = "Update",
}

// Placeholder variables supported by the MKV output file template. These are
// literal tokens, so they are intentionally not translated.
const MKV_OUTPUT_TEMPLATE_VARIABLES =
  "{file_name}, {video_count}, {video_codec_1}, {audio_count}, {audio_codec_1}, {text_count}, {text_codec_1}";

function SectionHeader({ icon, title }: { icon: React.ReactNode; title: string }) {
  return (
    <Box sx={{ display: "flex", alignItems: "center", gap: 1, mb: 2 }}>
      <Box sx={{ color: "primary.main", display: "flex" }}>{icon}</Box>
      <Typography variant="subtitle1" sx={{ fontWeight: 600 }}>
        {title}
      </Typography>
    </Box>
  );
}

function SettingRow({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <Box
      sx={{
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: 2,
        py: 1,
        "&:not(:last-child)": { borderBottom: 1, borderColor: "divider" },
      }}
    >
      <Typography variant="body2" color="text.secondary">
        {label}
      </Typography>
      <Box>{children}</Box>
    </Box>
  );
}

export default function Config() {
  const { t } = useTranslation();
  const config = useAppStore((s) => s.config);
  const setConfigState = useAppStore((s) => s.setConfig);
  const setNotification = useAppStore((s) => s.setDialogNotification);

  const [mainTab, setMainTab] = useState<ConfigTab>(ConfigTab.Appearance);
  const [draft, setDraft] = useState<Protocol.Config | null>(config);
  const [mkvtoolnixFound, setMkvtoolnixFound] = useState(false);
  const [betterMediaInfoFound, setBetterMediaInfoFound] = useState(false);
  const [mpcHcFound, setMpcHcFound] = useState(false);
  const isInitializedRef = useRef(false);
  const mkvToolNixCheckDebounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const betterMediaInfoCheckDebounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const mpcHcCheckDebounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const isWindows = typeof navigator !== "undefined" && /Windows/i.test(navigator.userAgent);

  useEffect(() => {
    if (config && !isInitializedRef.current) {
      setDraft(config);
      isInitializedRef.current = true;
    }
  }, [config]);

  // Push appearance / theme / language to the store immediately so the rest
  // of the UI re-themes and re-translates without waiting for the debounced
  // disk save.
  useEffect(() => {
    if (!isInitializedRef.current || !draft || !config) {
      return;
    }
    if (
      draft.displayMode !== config.displayMode ||
      draft.theme !== config.theme ||
      draft.language !== config.language
    ) {
      setConfigState({
        ...config,
        displayMode: draft.displayMode,
        theme: draft.theme,
        language: draft.language,
      });
    }
  }, [draft?.displayMode, draft?.theme, draft?.language, config, setConfigState]);

  // Apply i18n language change immediately.
  useEffect(() => {
    if (!isInitializedRef.current || !draft) {
      return;
    }
    changeLanguage(draft.language);
  }, [draft?.language]);

  // Auto-save: persist the entire draft to disk shortly after any change.
  // Debounced so text-input edits don't write on every keystroke.
  useEffect(() => {
    if (!isInitializedRef.current || !draft) {
      return;
    }
    const handle = setTimeout(async () => {
      try {
        const saved = await saveConfig(draft);
        setConfigState(saved);
      } catch (error) {
        setNotification({
          title: `${t("settings.settingsSaveError")} ${error}`,
          type: Protocol.DialogNotificationType.Error,
        });
      }
    }, 300);
    return () => clearTimeout(handle);
  }, [draft, setConfigState, setNotification, t]);

  // Validate the configured MKVToolNix path. The backend may auto-detect on
  // macOS and return a corrected path; mirror that into the draft when it
  // happens so the user sees the resolved location.
  useEffect(() => {
    if (!isInitializedRef.current || !draft) {
      return;
    }
    const path = draft.integration?.mkv?.mkvToolNixPath ?? "";
    if (mkvToolNixCheckDebounceRef.current) {
      clearTimeout(mkvToolNixCheckDebounceRef.current);
    }
    let isCancelled = false;
    mkvToolNixCheckDebounceRef.current = setTimeout(async () => {
      try {
        const status = await isMkvtoolnixFound(path.trim());
        if (!isCancelled) {
          setMkvtoolnixFound(status.found);
          if (
            status.found &&
            status.mkvToolNixPath &&
            status.mkvToolNixPath !== path
          ) {
            setDraft((d) =>
              d
                ? {
                    ...d,
                    integration: {
                      ...d.integration,
                      mkv: { ...d.integration.mkv, mkvToolNixPath: status.mkvToolNixPath },
                    },
                  }
                : d
            );
          }
        }
      } catch {
        if (!isCancelled) {
          setMkvtoolnixFound(false);
        }
      }
    }, 250);
    return () => {
      isCancelled = true;
      if (mkvToolNixCheckDebounceRef.current) {
        clearTimeout(mkvToolNixCheckDebounceRef.current);
      }
    };
  }, [draft?.integration?.mkv?.mkvToolNixPath]);

  // Validate the configured MPC-HC path (Windows only).
  useEffect(() => {
    if (!isInitializedRef.current || !draft || !isWindows) {
      return;
    }
    const path = draft.integration?.mpchc?.path ?? "";
    if (mpcHcCheckDebounceRef.current) {
      clearTimeout(mpcHcCheckDebounceRef.current);
    }
    let isCancelled = false;
    mpcHcCheckDebounceRef.current = setTimeout(async () => {
      try {
        const status = await isMpcHcFound(path.trim());
        if (!isCancelled) {
          setMpcHcFound(status.found);
          if (status.found && status.path && status.path !== path) {
            setDraft((d) =>
              d
                ? {
                    ...d,
                    integration: { ...d.integration, mpchc: { path: status.path } },
                  }
                : d
            );
          }
        }
      } catch {
        if (!isCancelled) {
          setMpcHcFound(false);
        }
      }
    }, 250);
    return () => {
      isCancelled = true;
      if (mpcHcCheckDebounceRef.current) {
        clearTimeout(mpcHcCheckDebounceRef.current);
      }
    };
  }, [draft?.integration?.mpchc?.path, isWindows]);

  // Validate the configured BetterMediaInfo path. Mirrors the same debounce +
  // auto-correct pattern used for MKVToolNix above.
  useEffect(() => {
    if (!isInitializedRef.current || !draft) {
      return;
    }
    const path = draft.integration?.betterMediaInfo?.path ?? "";
    if (betterMediaInfoCheckDebounceRef.current) {
      clearTimeout(betterMediaInfoCheckDebounceRef.current);
    }
    let isCancelled = false;
    betterMediaInfoCheckDebounceRef.current = setTimeout(async () => {
      try {
        const status = await isBetterMediaInfoFound(path.trim());
        if (!isCancelled) {
          setBetterMediaInfoFound(status.found);
          if (status.found && status.path && status.path !== path) {
            setDraft((d) =>
              d
                ? {
                    ...d,
                    integration: {
                      ...d.integration,
                      betterMediaInfo: { path: status.path },
                    },
                  }
                : d
            );
          }
        }
      } catch {
        if (!isCancelled) {
          setBetterMediaInfoFound(false);
        }
      }
    }, 250);
    return () => {
      isCancelled = true;
      if (betterMediaInfoCheckDebounceRef.current) {
        clearTimeout(betterMediaInfoCheckDebounceRef.current);
      }
    };
  }, [draft?.integration?.betterMediaInfo?.path]);

  if (!draft) {
    return <Box sx={{ p: 2 }}>{t("common.loading")}</Box>;
  }

  const updateDraft = (patch: Partial<Protocol.Config>) => {
    setDraft({ ...draft, ...patch } as Protocol.Config);
  };

  const updateScan = (patch: Partial<Protocol.ConfigScan>) => {
    setDraft({ ...draft, scan: { ...draft.scan, ...patch } });
  };

  const updateFormatting = (patch: Partial<Protocol.ConfigFormatting>) => {
    setDraft({ ...draft, formatting: { ...draft.formatting, ...patch } });
  };

  const updateUpdate = (patch: Partial<Protocol.ConfigUpdate>) => {
    setDraft({ ...draft, update: { ...draft.update, ...patch } });
  };

  const updateMkv = (patch: Partial<Protocol.ConfigMkv>) => {
    setDraft({
      ...draft,
      integration: { ...draft.integration, mkv: { ...draft.integration.mkv, ...patch } },
    });
  };

  const updateBetterMediaInfo = (patch: Partial<Protocol.ConfigBetterMediaInfo>) => {
    setDraft({
      ...draft,
      integration: {
        ...draft.integration,
        betterMediaInfo: { ...draft.integration.betterMediaInfo, ...patch },
      },
    });
  };

  const updateMpcHc = (patch: Partial<Protocol.ConfigMpcHc>) => {
    setDraft({
      ...draft,
      integration: { ...draft.integration, mpchc: { ...draft.integration.mpchc, ...patch } },
    });
  };

  const handleBrowseMkvToolNixPath = async () => {
    const directory = await openDialog({
      directory: true,
      defaultPath: draft.integration.mkv?.mkvToolNixPath?.trim() || undefined,
    });
    if (typeof directory === "string" && directory.length > 0) {
      updateMkv({ mkvToolNixPath: directory });
    }
  };

  const handleDetectMkvToolNix = async () => {
    try {
      const status = await isMkvtoolnixFound(
        draft.integration.mkv?.mkvToolNixPath?.trim() ?? "",
        true
      );
      setMkvtoolnixFound(status.found);
      if (
        status.found &&
        status.mkvToolNixPath &&
        status.mkvToolNixPath !== draft.integration.mkv?.mkvToolNixPath
      ) {
        updateMkv({ mkvToolNixPath: status.mkvToolNixPath });
      }
    } catch {
      setMkvtoolnixFound(false);
    }
  };

  const handleBrowseBetterMediaInfoPath = async () => {
    const directory = await openDialog({
      directory: true,
      defaultPath: draft.integration.betterMediaInfo?.path?.trim() || undefined,
    });
    if (typeof directory === "string" && directory.length > 0) {
      updateBetterMediaInfo({ path: directory });
    }
  };

  const handleDetectBetterMediaInfo = async () => {
    try {
      const status = await isBetterMediaInfoFound(
        draft.integration.betterMediaInfo?.path?.trim() ?? "",
        true
      );
      setBetterMediaInfoFound(status.found);
      if (
        status.found &&
        status.path &&
        status.path !== draft.integration.betterMediaInfo?.path
      ) {
        updateBetterMediaInfo({ path: status.path });
      }
    } catch {
      setBetterMediaInfoFound(false);
    }
  };

  const handleBrowseMpcHcPath = async () => {
    const file = await openDialog({
      multiple: false,
      defaultPath: draft.integration.mpchc?.path?.trim() || undefined,
      filters: [{ name: "MPC-HC", extensions: ["exe"] }],
    });
    if (typeof file === "string" && file.length > 0) {
      updateMpcHc({ path: file });
    }
  };

  const handleDetectMpcHc = async () => {
    try {
      const status = await isMpcHcFound(draft.integration.mpchc?.path?.trim() ?? "", true);
      setMpcHcFound(status.found);
      if (status.found && status.path && status.path !== draft.integration.mpchc?.path) {
        updateMpcHc({ path: status.path });
      }
    } catch {
      setMpcHcFound(false);
    }
  };

  const updateFormattingBitRate = (patch: Partial<Protocol.ConfigBitRate>) => {
    updateFormatting({ bitRate: { ...draft.formatting.bitRate, ...patch } });
  };
  const updateFormattingSize = (patch: Partial<Protocol.ConfigSize>) => {
    updateFormatting({ size: { ...draft.formatting.size, ...patch } });
  };

  const getThemeLabel = (theme: Protocol.Theme) =>
    t(`settings.themeNames.${theme}`, { defaultValue: theme });

  const appearancePanel = (
    <Box>
      <SectionHeader icon={<AppearanceIcon fontSize="small" />} title={t("settings.appearance")} />
      <SettingRow label={t("settings.mode")}>
        <ToggleButtonGroup
          exclusive
          size="small"
          value={draft.displayMode}
          onChange={(_, v) => v && updateDraft({ displayMode: v })}
          sx={{ "& .MuiToggleButton-root": { textTransform: "none" } }}
        >
          <ToggleButton value={Protocol.DisplayMode.Auto}>
            <AutoIcon fontSize="small" sx={{ mr: 0.5 }} />
            {t("settings.autoMode")}
          </ToggleButton>
          <ToggleButton value={Protocol.DisplayMode.Light}>
            <LightIcon fontSize="small" sx={{ mr: 0.5 }} />
            {t("settings.lightMode")}
          </ToggleButton>
          <ToggleButton value={Protocol.DisplayMode.Dark}>
            <DarkIcon fontSize="small" sx={{ mr: 0.5 }} />
            {t("settings.darkMode")}
          </ToggleButton>
        </ToggleButtonGroup>
      </SettingRow>
      <SettingRow label={t("settings.theme")}>
        <FormControl size="small" sx={{ minWidth: 160 }}>
          <Select
            value={draft.theme}
            onChange={(e) => updateDraft({ theme: e.target.value as Protocol.Theme })}
          >
            {Protocol.getThemes().map((th) => (
              <MenuItem key={th} value={th}>
                {getThemeLabel(th)}
              </MenuItem>
            ))}
          </Select>
        </FormControl>
      </SettingRow>
      <SettingRow label={t("settings.language")}>
        <FormControl size="small" sx={{ minWidth: 200 }}>
          <Select
            value={draft.language}
            onChange={(e) => updateDraft({ language: e.target.value as Protocol.Language })}
          >
            {Protocol.getLanguages().map((lang) => (
              <MenuItem key={lang} value={lang}>
                {Protocol.getLanguageLabel(lang)}
              </MenuItem>
            ))}
          </Select>
        </FormControl>
      </SettingRow>
    </Box>
  );

  const scanPanel = (
    <Box>
      <SectionHeader icon={<ScanIcon fontSize="small" />} title={t("settings.scan")} />
      <Stack>
        <SettingRow label={t("settings.enableSsifSupport")}>
          <Switch
            checked={draft.scan.enableSsifSupport}
            onChange={(e) => updateScan({ enableSsifSupport: e.target.checked })}
          />
        </SettingRow>
        <SettingRow label={t("settings.filterLoopingPlaylists")}>
          <Switch
            checked={draft.scan.filterLoopingPlaylists}
            onChange={(e) => updateScan({ filterLoopingPlaylists: e.target.checked })}
          />
        </SettingRow>
        <SettingRow label={t("settings.filterShortPlaylists")}>
          <Stack direction="row" spacing={1} sx={{ alignItems: "center" }}>
            <Switch
              checked={draft.scan.filterShortPlaylists}
              onChange={(e) => updateScan({ filterShortPlaylists: e.target.checked })}
            />
            <FormControlLabel
              control={
                <TextField
                  size="small"
                  type="number"
                  sx={{ width: 80 }}
                  value={draft.scan.filterShortPlaylistsValue}
                  onChange={(e) =>
                    updateScan({ filterShortPlaylistsValue: parseInt(e.target.value || "0", 10) })
                  }
                  disabled={!draft.scan.filterShortPlaylists}
                />
              }
              label={t("settings.filterShortPlaylistsValue")}
              labelPlacement="start"
              sx={{ ml: 1 }}
            />
          </Stack>
        </SettingRow>
      </Stack>
    </Box>
  );

  const formattingPanel = (
    <Box>
      <SectionHeader icon={<FormatIcon fontSize="small" />} title={t("settings.formatting")} />
      <Stack>
        <Typography variant="body2" sx={{ fontWeight: 500, mb: 1 }}>
          {t("settings.bitRate")}
        </Typography>
        <Box sx={{ display: "flex", gap: 2, mb: 2 }}>
          <Box sx={{ flex: 1 }}>
            <Typography variant="caption" color="text.secondary">
              {t("settings.precision")}
            </Typography>
            <FormControl size="small" fullWidth sx={{ mt: 0.5 }}>
              <Select
                value={draft.formatting.bitRate.precision}
                onChange={(e) =>
                  updateFormattingBitRate({ precision: e.target.value as Protocol.FormatPrecision })
                }
              >
                {Protocol.getFormatPrecisions().map((p) => (
                  <MenuItem key={p} value={p}>
                    {Protocol.getFormatPrecisionLabel(p)}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </Box>
          <Box sx={{ flex: 1 }}>
            <Typography variant="caption" color="text.secondary">
              {t("settings.unit")}
            </Typography>
            <FormControl size="small" fullWidth sx={{ mt: 0.5 }}>
              <Select
                value={draft.formatting.bitRate.unit}
                onChange={(e) =>
                  updateFormattingBitRate({ unit: e.target.value as Protocol.FormatUnit })
                }
              >
                {Protocol.getFormatUnits().map((u) => (
                  <MenuItem key={u} value={u}>
                    {Protocol.getFormatUnitLabel(u)}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </Box>
        </Box>
        <Typography variant="body2" sx={{ fontWeight: 500, mb: 1 }}>
          {t("settings.size")}
        </Typography>
        <Box sx={{ display: "flex", gap: 2 }}>
          <Box sx={{ flex: 1 }}>
            <Typography variant="caption" color="text.secondary">
              {t("settings.precision")}
            </Typography>
            <FormControl size="small" fullWidth sx={{ mt: 0.5 }}>
              <Select
                value={draft.formatting.size.precision}
                onChange={(e) =>
                  updateFormattingSize({ precision: e.target.value as Protocol.FormatPrecision })
                }
              >
                {Protocol.getFormatPrecisions().map((p) => (
                  <MenuItem key={p} value={p}>
                    {Protocol.getFormatPrecisionLabel(p)}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </Box>
          <Box sx={{ flex: 1 }}>
            <Typography variant="caption" color="text.secondary">
              {t("settings.unit")}
            </Typography>
            <FormControl size="small" fullWidth sx={{ mt: 0.5 }}>
              <Select
                value={draft.formatting.size.unit}
                onChange={(e) =>
                  updateFormattingSize({ unit: e.target.value as Protocol.FormatUnit })
                }
              >
                {Protocol.getFormatUnits().map((u) => (
                  <MenuItem key={u} value={u}>
                    {Protocol.getFormatUnitLabel(u)}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </Box>
        </Box>
      </Stack>
    </Box>
  );

  const integrationPanel = (
    <Box>
      <SectionHeader icon={<IntegrationIcon fontSize="small" />} title={t("settings.integration")} />
      <Stack spacing={2}>
        {isWindows && (
          <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
            <SectionHeader
              icon={
                <Box
                  component="img"
                  src="images/mpchc64.png"
                  alt="MPC-HC"
                  sx={{ width: 20, height: 20, objectFit: "contain" }}
                />
              }
              title={t("settings.mpchc")}
            />
            <Box sx={{ py: 1 }}>
              <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
                {t("settings.mpchcPath")}
              </Typography>
              <Box sx={{ display: "flex", alignItems: "center", gap: 1 }}>
                <TextField
                  value={draft.integration.mpchc?.path ?? ""}
                  onChange={(e) => updateMpcHc({ path: e.target.value })}
                  size="small"
                  fullWidth
                />
                <Button
                  variant="outlined"
                  size="small"
                  onClick={handleBrowseMpcHcPath}
                  sx={{ minWidth: 90, height: 36, textTransform: "none" }}
                >
                  {t("settings.browse")}
                </Button>
                <Button
                  variant="outlined"
                  size="small"
                  onClick={handleDetectMpcHc}
                  sx={{ minWidth: 90, height: 36, textTransform: "none" }}
                >
                  {t("settings.detect")}
                </Button>
              </Box>
              <Typography
                variant="caption"
                sx={{
                  mt: 0.75,
                  display: "block",
                  color: mpcHcFound ? "success.main" : "error.main",
                }}
              >
                {mpcHcFound ? t("settings.mpchcFound") : t("settings.mpchcNotFound")}
              </Typography>
            </Box>
          </Paper>
        )}

        <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
          <SectionHeader
            icon={
              <Box
                component="img"
                src="images/mkvmerge.png"
                alt="MKVToolNix"
                sx={{ width: 20, height: 20, objectFit: "contain" }}
              />
            }
            title={t("settings.mkv")}
          />
          <Box sx={{ py: 1 }}>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
              {t("settings.mkvToolNixPath")}
            </Typography>
            <Box sx={{ display: "flex", alignItems: "center", gap: 1 }}>
              <TextField
                value={draft.integration.mkv?.mkvToolNixPath ?? ""}
                onChange={(e) => updateMkv({ mkvToolNixPath: e.target.value })}
                size="small"
                fullWidth
              />
              <Button
                variant="outlined"
                size="small"
                onClick={handleBrowseMkvToolNixPath}
                sx={{ minWidth: 90, height: 36, textTransform: "none" }}
              >
                {t("settings.browse")}
              </Button>
              <Button
                variant="outlined"
                size="small"
                onClick={handleDetectMkvToolNix}
                sx={{ minWidth: 90, height: 36, textTransform: "none" }}
              >
                {t("settings.detect")}
              </Button>
            </Box>
            <Typography
              variant="caption"
              sx={{
                mt: 0.75,
                display: "block",
                color: mkvtoolnixFound ? "success.main" : "error.main",
              }}
            >
              {mkvtoolnixFound
                ? t("settings.mkvtoolnixFound")
                : t("settings.mkvtoolnixNotFound")}
            </Typography>
          </Box>
          <Box sx={{ py: 1 }}>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
              {t("settings.outputFileTemplate")}
            </Typography>
            <TextField
              value={draft.integration.mkv?.outputFileTemplate ?? ""}
              onChange={(e) => updateMkv({ outputFileTemplate: e.target.value })}
              size="small"
              fullWidth
            />
            <Typography variant="caption" color="text.secondary" sx={{ mt: 0.75, display: "block" }}>
              {t("settings.placeholderVariables")} {MKV_OUTPUT_TEMPLATE_VARIABLES}
            </Typography>
          </Box>
        </Paper>

        <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
          <SectionHeader
            icon={
              <Box
                component="img"
                src="images/bettermediainfo.png"
                alt="BetterMediaInfo"
                sx={{ width: 20, height: 20, objectFit: "contain" }}
              />
            }
            title={t("settings.betterMediaInfo")}
          />
          <Box sx={{ py: 1 }}>
            <Typography variant="body2" color="text.secondary" sx={{ mb: 1 }}>
              {t("settings.betterMediaInfoPath")}
            </Typography>
            <Box sx={{ display: "flex", alignItems: "center", gap: 1 }}>
              <TextField
                value={draft.integration.betterMediaInfo?.path ?? ""}
                onChange={(e) => updateBetterMediaInfo({ path: e.target.value })}
                size="small"
                fullWidth
              />
              <Button
                variant="outlined"
                size="small"
                onClick={handleBrowseBetterMediaInfoPath}
                sx={{ minWidth: 90, height: 36, textTransform: "none" }}
              >
                {t("settings.browse")}
              </Button>
              <Button
                variant="outlined"
                size="small"
                onClick={handleDetectBetterMediaInfo}
                sx={{ minWidth: 90, height: 36, textTransform: "none" }}
              >
                {t("settings.detect")}
              </Button>
            </Box>
            <Typography
              variant="caption"
              sx={{
                mt: 0.75,
                display: "block",
                color: betterMediaInfoFound ? "success.main" : "error.main",
              }}
            >
              {betterMediaInfoFound
                ? t("settings.betterMediaInfoFound")
                : t("settings.betterMediaInfoNotFound")}
            </Typography>
          </Box>
        </Paper>
      </Stack>
    </Box>
  );

  const updatePanel = (
    <Box>
      <SectionHeader icon={<UpdateIcon fontSize="small" />} title={t("settings.update")} />
      <SettingRow label={t("settings.checkNewVersion")}>
        <FormControl size="small" sx={{ minWidth: 160 }}>
          <Select
            value={draft.update.checkInterval}
            onChange={(e) =>
              updateUpdate({ checkInterval: e.target.value as Protocol.UpdateCheckInterval })
            }
          >
            <MenuItem value={Protocol.UpdateCheckInterval.Daily}>{t("settings.daily")}</MenuItem>
            <MenuItem value={Protocol.UpdateCheckInterval.Weekly}>{t("settings.weekly")}</MenuItem>
            <MenuItem value={Protocol.UpdateCheckInterval.Monthly}>{t("settings.monthly")}</MenuItem>
          </Select>
        </FormControl>
      </SettingRow>
    </Box>
  );

  return (
    <Box
      sx={{
        width: "100%",
        maxWidth: 900,
        mx: "auto",
        py: 2,
        px: 1,
        display: "flex",
        gap: 2,
        height: "100%",
        minHeight: 0,
      }}
    >
      <Tabs
        orientation="vertical"
        value={mainTab}
        onChange={(_e, v: ConfigTab) => setMainTab(v)}
        sx={{
          borderRight: 1,
          borderColor: "divider",
          minWidth: 180,
          "& .MuiTab-root": {
            minHeight: 40,
            alignItems: "center",
            justifyContent: "flex-start",
            textAlign: "left",
            textTransform: "none",
          },
        }}
      >
        <Tab
          value={ConfigTab.Appearance}
          icon={<AppearanceIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t("settings.appearance")}
        />
        <Tab
          value={ConfigTab.Scan}
          icon={<ScanIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t("settings.scan")}
        />
        <Tab
          value={ConfigTab.Formatting}
          icon={<FormatIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t("settings.formatting")}
        />
        <Tab
          value={ConfigTab.Integration}
          icon={<IntegrationIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t("settings.integration")}
        />
        <Tab
          value={ConfigTab.Update}
          icon={<UpdateIcon sx={{ fontSize: 18 }} />}
          iconPosition="start"
          label={t("settings.update")}
        />
      </Tabs>
      <Box
        sx={{
          flex: 1,
          minWidth: 0,
          minHeight: 0,
          display: "flex",
          flexDirection: "column",
          overflow: "auto",
        }}
      >
        {mainTab === ConfigTab.Appearance && appearancePanel}
        {mainTab === ConfigTab.Scan && scanPanel}
        {mainTab === ConfigTab.Formatting && formattingPanel}
        {mainTab === ConfigTab.Integration && integrationPanel}
        {mainTab === ConfigTab.Update && updatePanel}
      </Box>
    </Box>
  );
}
