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
  TextField,
  ToggleButton,
  ToggleButtonGroup,
  Typography,
} from "@mui/material";
import {
  BrightnessAuto as AutoIcon,
  DarkMode as DarkIcon,
  LightMode as LightIcon,
  Palette as AppearanceIcon,
  Tune as ScanIcon,
  Numbers as FormatIcon,
} from "@mui/icons-material";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useTranslation } from "react-i18next";
import * as Protocol from "../lib/protocol";
import {
  isBetterMediaInfoFound,
  isMkvtoolnixFound,
  setConfig as saveConfig,
} from "../lib/service";
import { useAppStore } from "../lib/store";
import { changeLanguage } from "../i18n";

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

  const [draft, setDraft] = useState<Protocol.Config | null>(config);
  const [mkvtoolnixFound, setMkvtoolnixFound] = useState(false);
  const [betterMediaInfoFound, setBetterMediaInfoFound] = useState(false);
  const isInitializedRef = useRef(false);
  const mkvToolNixCheckDebounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const betterMediaInfoCheckDebounceRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

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
    if (!isInitializedRef.current || !draft || !config) return;
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
    if (!isInitializedRef.current || !draft) return;
    changeLanguage(draft.language);
  }, [draft?.language]);

  // Auto-save: persist the entire draft to disk shortly after any change.
  // Debounced so text-input edits don't write on every keystroke.
  useEffect(() => {
    if (!isInitializedRef.current || !draft) return;
    const handle = setTimeout(async () => {
      try {
        const saved = await saveConfig(draft);
        setConfigState(saved);
        changeLanguage(saved.language);
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
    if (!isInitializedRef.current || !draft) return;
    const path = draft.mkv?.mkvToolNixPath ?? "";
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
              d ? { ...d, mkv: { mkvToolNixPath: status.mkvToolNixPath } } : d
            );
          }
        }
      } catch {
        if (!isCancelled) setMkvtoolnixFound(false);
      }
    }, 250);
    return () => {
      isCancelled = true;
      if (mkvToolNixCheckDebounceRef.current) {
        clearTimeout(mkvToolNixCheckDebounceRef.current);
      }
    };
  }, [draft?.mkv?.mkvToolNixPath]);

  // Validate the configured BetterMediaInfo path. Mirrors the same debounce +
  // auto-correct pattern used for MKVToolNix above.
  useEffect(() => {
    if (!isInitializedRef.current || !draft) return;
    const path = draft.betterMediaInfo?.path ?? "";
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
              d ? { ...d, betterMediaInfo: { path: status.path } } : d
            );
          }
        }
      } catch {
        if (!isCancelled) setBetterMediaInfoFound(false);
      }
    }, 250);
    return () => {
      isCancelled = true;
      if (betterMediaInfoCheckDebounceRef.current) {
        clearTimeout(betterMediaInfoCheckDebounceRef.current);
      }
    };
  }, [draft?.betterMediaInfo?.path]);

  if (!draft) {
    return <Box sx={{ p: 2 }}>Loading…</Box>;
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
  const updateMkv = (patch: Partial<Protocol.ConfigMkv>) => {
    setDraft({ ...draft, mkv: { ...draft.mkv, ...patch } });
  };

  const handleBrowseMkvToolNixPath = async () => {
    const directory = await openDialog({
      directory: true,
      defaultPath: draft.mkv?.mkvToolNixPath?.trim() || undefined,
    });
    if (typeof directory === "string" && directory.length > 0) {
      updateMkv({ mkvToolNixPath: directory });
    }
  };

  const handleDetectMkvToolNix = async () => {
    try {
      const status = await isMkvtoolnixFound(
        draft.mkv?.mkvToolNixPath?.trim() ?? "",
        true
      );
      setMkvtoolnixFound(status.found);
      if (
        status.found &&
        status.mkvToolNixPath &&
        status.mkvToolNixPath !== draft.mkv?.mkvToolNixPath
      ) {
        updateMkv({ mkvToolNixPath: status.mkvToolNixPath });
      }
    } catch {
      setMkvtoolnixFound(false);
    }
  };

  const updateBetterMediaInfo = (patch: Partial<Protocol.ConfigBetterMediaInfo>) => {
    setDraft({ ...draft, betterMediaInfo: { ...draft.betterMediaInfo, ...patch } });
  };

  const handleBrowseBetterMediaInfoPath = async () => {
    const directory = await openDialog({
      directory: true,
      defaultPath: draft.betterMediaInfo?.path?.trim() || undefined,
    });
    if (typeof directory === "string" && directory.length > 0) {
      updateBetterMediaInfo({ path: directory });
    }
  };

  const handleDetectBetterMediaInfo = async () => {
    try {
      const status = await isBetterMediaInfoFound(
        draft.betterMediaInfo?.path?.trim() ?? "",
        true
      );
      setBetterMediaInfoFound(status.found);
      if (
        status.found &&
        status.path &&
        status.path !== draft.betterMediaInfo?.path
      ) {
        updateBetterMediaInfo({ path: status.path });
      }
    } catch {
      setBetterMediaInfoFound(false);
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

  return (
    <Box sx={{ width: "100%", maxWidth: 640, mx: "auto", py: 2, px: 1, display: "flex", flexDirection: "column", gap: 2 }}>
      <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
        <SectionHeader icon={<AppearanceIcon />} title={t("settings.appearance")} />
        <SettingRow label={t("settings.appearance")}>
          <ToggleButtonGroup
            exclusive
            size="small"
            value={draft.displayMode}
            onChange={(_, v) => v && updateDraft({ displayMode: v })}
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
      </Paper>

      <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
        <SectionHeader icon={<ScanIcon />} title={t("settings.scan")} />
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
          <SettingRow label={t("settings.useImagePrefix")}>
            <Stack direction="row" spacing={1} sx={{ alignItems: "center" }}>
              <Switch
                checked={draft.scan.useImagePrefix}
                onChange={(e) => updateScan({ useImagePrefix: e.target.checked })}
              />
              <TextField
                size="small"
                placeholder={t("settings.useImagePrefixValue")}
                value={draft.scan.useImagePrefixValue}
                onChange={(e) => updateScan({ useImagePrefixValue: e.target.value })}
                disabled={!draft.scan.useImagePrefix}
                sx={{ width: 200 }}
              />
            </Stack>
          </SettingRow>
        </Stack>
      </Paper>

      <Paper variant="outlined" sx={{ p: 2, borderRadius: 2 }}>
        <SectionHeader icon={<FormatIcon />} title={t("settings.formatting")} />
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
      </Paper>

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
              value={draft.mkv?.mkvToolNixPath ?? ""}
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
              value={draft.betterMediaInfo?.path ?? ""}
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

    </Box>
  );
}
