/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import {
  Avatar,
  Box,
  Card,
  CardActionArea,
  CardContent,
  Chip,
  Stack,
  Typography,
} from "@mui/material";
import GitHubIcon from "@mui/icons-material/GitHub";
import PersonIcon from "@mui/icons-material/Person";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { useTranslation } from "react-i18next";

import appIconUrl from "../../src-tauri/icons/icon.png";
import { useAppStore } from "../lib/store";

const APP_NAME = "BDMaster";
const AUTHOR_NAME = "Sam Cao";
const AUTHOR_URL = "https://github.com/caoccao";
const GITHUB_URL = "https://github.com/caoccao/BDMaster";
const GRADIENT = "linear-gradient(135deg, #6366f1 0%, #ec4899 100%)";

export default function About() {
  const { t } = useTranslation();
  const about = useAppStore((s) => s.about);
  const appVersion = about?.appVersion ?? "";

  const infoCardSx = {
    border: 1,
    borderColor: "divider",
    borderRadius: 3,
    transition: "transform 0.2s, box-shadow 0.2s, border-color 0.2s",
    "&:hover": {
      transform: "translateY(-2px)",
      boxShadow: "0 8px 24px rgba(99, 102, 241, 0.18)",
      borderColor: "primary.main",
    },
  };

  const labelSx = {
    textTransform: "uppercase" as const,
    letterSpacing: 1,
    fontWeight: 600,
  };

  return (
    <Box sx={{ display: "grid", gap: 2 }}>
      <Box sx={{ maxWidth: 640, mx: "auto", px: 2, py: 3, width: "100%" }}>
        <Stack spacing={4} sx={{ alignItems: "center" }}>
          <Stack spacing={1.5} sx={{ alignItems: "center" }}>
            <Box
              sx={{
                position: "relative",
                width: 96,
                height: 96,
                display: "flex",
                alignItems: "center",
                justifyContent: "center",
                "&::before": {
                  content: '""',
                  position: "absolute",
                  inset: -16,
                  borderRadius: "50%",
                  background:
                    "radial-gradient(circle, rgba(99, 102, 241, 0.35) 0%, rgba(236, 72, 153, 0.15) 50%, transparent 75%)",
                  zIndex: 0,
                },
              }}
            >
              <Box
                component="img"
                src={appIconUrl}
                alt={APP_NAME}
                sx={{
                  position: "relative",
                  zIndex: 1,
                  width: 96,
                  height: 96,
                  filter: "drop-shadow(0 8px 20px rgba(99, 102, 241, 0.35))",
                }}
              />
            </Box>
            <Typography
              variant="h3"
              sx={{
                fontWeight: 800,
                letterSpacing: "-0.02em",
                backgroundImage: GRADIENT,
                backgroundClip: "text",
                WebkitBackgroundClip: "text",
                color: "transparent",
                textAlign: "center",
              }}
            >
              {APP_NAME}
            </Typography>
            <Stack direction="row" spacing={1} sx={{ flexWrap: "wrap", justifyContent: "center" }}>
              {appVersion && (
                <Chip
                  label={`v${appVersion}`}
                  size="small"
                  variant="outlined"
                  sx={{ fontWeight: 600, letterSpacing: 0.5 }}
                />
              )}
            </Stack>
            <Typography variant="body2" color="text.secondary" sx={{ textAlign: "center" }}>
              {t("about.tagline")}
            </Typography>
          </Stack>

          <Stack
            direction="row"
            spacing={2}
            sx={{ width: "100%", alignItems: "stretch" }}
          >
            <Card elevation={0} sx={{ ...infoCardSx, flexShrink: 0 }}>
              <CardActionArea onClick={() => shellOpen(AUTHOR_URL)} sx={{ height: "100%" }}>
                <CardContent sx={{ display: "flex", alignItems: "center", gap: 2 }}>
                  <Avatar sx={{ background: GRADIENT, width: 48, height: 48 }}>
                    <PersonIcon />
                  </Avatar>
                  <Box sx={{ minWidth: 0 }}>
                    <Typography variant="caption" color="text.secondary" sx={labelSx}>
                      {t("about.author")}
                    </Typography>
                    <Typography variant="h6" sx={{ lineHeight: 1.2, whiteSpace: "nowrap" }}>
                      {AUTHOR_NAME}
                    </Typography>
                  </Box>
                </CardContent>
              </CardActionArea>
            </Card>

            <Card elevation={0} sx={{ ...infoCardSx, flex: 1, minWidth: 0 }}>
              <CardActionArea onClick={() => shellOpen(GITHUB_URL)} sx={{ height: "100%" }}>
                <CardContent sx={{ display: "flex", alignItems: "center", gap: 2 }}>
                  <Avatar sx={{ bgcolor: "#24292f", width: 48, height: 48 }}>
                    <GitHubIcon sx={{ color: "#fff" }} />
                  </Avatar>
                  <Box sx={{ minWidth: 0, flex: 1 }}>
                    <Typography variant="caption" color="text.secondary" sx={labelSx}>
                      {t("about.github")}
                    </Typography>
                    <Typography
                      variant="body2"
                      sx={{
                        fontFamily: "ui-monospace, SFMono-Regular, Menlo, Consolas, monospace",
                        whiteSpace: "nowrap",
                        overflow: "hidden",
                        textOverflow: "ellipsis",
                      }}
                    >
                      {GITHUB_URL}
                    </Typography>
                  </Box>
                </CardContent>
              </CardActionArea>
            </Card>
          </Stack>
        </Stack>
      </Box>
    </Box>
  );
}
