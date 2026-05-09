/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { useEffect, useRef, useState } from "react";
import {
  Box,
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
import { useTranslation } from "react-i18next";
import * as Protocol from "../lib/protocol";
import { setConfig as saveConfig } from "../lib/service";
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
  const isInitializedRef = useRef(false);

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
          <SettingRow label={t("settings.generateStreamDiagnostics")}>
            <Switch
              checked={draft.scan.generateStreamDiagnostics}
              onChange={(e) => updateScan({ generateStreamDiagnostics: e.target.checked })}
            />
          </SettingRow>
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
          <SettingRow label={t("settings.keepStreamOrder")}>
            <Switch
              checked={draft.scan.keepStreamOrder}
              onChange={(e) => updateScan({ keepStreamOrder: e.target.checked })}
            />
          </SettingRow>
          <SettingRow label={t("settings.generateTextSummary")}>
            <Switch
              checked={draft.scan.generateTextSummary}
              onChange={(e) => updateScan({ generateTextSummary: e.target.checked })}
            />
          </SettingRow>
          <SettingRow label={t("settings.autosaveReport")}>
            <Switch
              checked={draft.scan.autosaveReport}
              onChange={(e) => updateScan({ autosaveReport: e.target.checked })}
            />
          </SettingRow>
          <SettingRow label={t("settings.displayChapterCount")}>
            <Switch
              checked={draft.scan.displayChapterCount}
              onChange={(e) => updateScan({ displayChapterCount: e.target.checked })}
            />
          </SettingRow>
          <SettingRow label={t("settings.enableExtendedStreamDiagnostics")}>
            <Switch
              checked={draft.scan.enableExtendedStreamDiagnostics}
              onChange={(e) => updateScan({ enableExtendedStreamDiagnostics: e.target.checked })}
            />
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

    </Box>
  );
}
