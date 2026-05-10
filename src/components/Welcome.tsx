/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import {
  Box,
  Button,
  Link,
  Stack,
  Typography,
} from "@mui/material";
import AlbumIcon from "@mui/icons-material/Album";
import FolderIcon from "@mui/icons-material/Folder";
import GitHubIcon from "@mui/icons-material/GitHub";
import PersonIcon from "@mui/icons-material/Person";
import { useTranslation } from "react-i18next";
import { open as shellOpen } from "@tauri-apps/plugin-shell";
import { openDiscDirectoryDialog, openDiscFileDialog } from "../lib/dialog";

const AUTHOR_NAME = "Sam Cao";
const AUTHOR_URL = "https://github.com/caoccao";
const BDMASTER_URL = "https://github.com/caoccao/BDMaster";
const BETTER_MEDIA_INFO_URL = "https://github.com/caoccao/BetterMediaInfo";
const BATCH_MKV_EXTRACT_URL = "https://github.com/caoccao/BatchMkvExtract";

interface AppCardProps {
  logo: string;
  title: string;
  intro: string;
  githubUrl: string;
  isPrimary?: boolean;
}

function AppCard({ logo, title, intro, githubUrl, isPrimary }: AppCardProps) {
  const { t } = useTranslation();
  return (
    <Box
      sx={(theme) => ({
        flex: 1,
        minWidth: 260,
        p: 3,
        borderRadius: 3,
        border: "1px solid",
        borderColor:
          theme.palette.mode === "dark"
            ? "rgba(96,165,250,0.35)"
            : "rgba(37,99,235,0.25)",
        background: isPrimary
          ? theme.palette.mode === "dark"
            ? "linear-gradient(140deg, rgba(37,99,235,0.32) 0%, rgba(14,165,233,0.18) 100%)"
            : "linear-gradient(140deg, rgba(59,130,246,0.16) 0%, rgba(14,165,233,0.10) 100%)"
          : theme.palette.mode === "dark"
            ? "linear-gradient(140deg, rgba(30,58,138,0.28) 0%, rgba(15,23,42,0.40) 100%)"
            : "linear-gradient(140deg, rgba(219,234,254,0.85) 0%, rgba(241,245,249,0.85) 100%)",
        boxShadow:
          theme.palette.mode === "dark"
            ? "0 10px 30px rgba(2,6,23,0.45)"
            : "0 10px 30px rgba(37,99,235,0.10)",
        display: "flex",
        flexDirection: "column",
        gap: 1.5,
        transition: "transform 160ms ease, box-shadow 160ms ease",
        "&:hover": {
          transform: "translateY(-2px)",
          boxShadow:
            theme.palette.mode === "dark"
              ? "0 14px 36px rgba(2,6,23,0.55)"
              : "0 14px 36px rgba(37,99,235,0.18)",
        },
      })}
    >
      <Box sx={{ display: "flex", alignItems: "center", gap: 2 }}>
        <Box
          component="img"
          src={logo}
          alt={title}
          sx={{
            width: 56,
            height: 56,
            borderRadius: 2,
            objectFit: "contain",
            backgroundColor: "rgba(255,255,255,0.6)",
            p: 0.5,
            boxShadow: "0 4px 12px rgba(15,23,42,0.12)",
          }}
        />
        <Typography
          variant="h6"
          sx={(theme) => ({
            fontWeight: 700,
            color: theme.palette.mode === "dark" ? "#bfdbfe" : "#1d4ed8",
          })}
        >
          {title}
        </Typography>
      </Box>
      <Typography variant="body2" color="text.secondary" sx={{ lineHeight: 1.6 }}>
        {intro}
      </Typography>
      <Box sx={{ flex: 1 }} />
      <Box>
        <Button
          size="small"
          startIcon={<GitHubIcon />}
          onClick={() => shellOpen(githubUrl)}
          sx={(theme) => ({
            textTransform: "none",
            color: theme.palette.mode === "dark" ? "#93c5fd" : "#1d4ed8",
            "&:hover": {
              backgroundColor:
                theme.palette.mode === "dark"
                  ? "rgba(59,130,246,0.16)"
                  : "rgba(37,99,235,0.08)",
            },
          })}
        >
          {t("cards.viewOnGithub")}
        </Button>
      </Box>
    </Box>
  );
}

export default function Welcome() {
  const { t } = useTranslation();
  return (
    <Box
      sx={(theme) => ({
        flex: 1,
        minHeight: 0,
        display: "flex",
        justifyContent: "center",
        alignItems: "flex-start",
        py: 4,
        px: 2,
        background:
          theme.palette.mode === "dark"
            ? "radial-gradient(circle at 20% 0%, rgba(30,64,175,0.20), transparent 60%), radial-gradient(circle at 80% 100%, rgba(14,165,233,0.16), transparent 55%)"
            : "radial-gradient(circle at 20% 0%, rgba(191,219,254,0.55), transparent 60%), radial-gradient(circle at 80% 100%, rgba(186,230,253,0.45), transparent 55%)",
        borderRadius: 2,
        overflow: "auto",
      })}
    >
      <Stack spacing={3} sx={{ width: "100%", maxWidth: 1100 }}>
        <Box sx={{ textAlign: "center" }}>
          <Typography
            variant="h4"
            sx={(theme) => ({
              fontWeight: 800,
              letterSpacing: "-0.02em",
              background:
                theme.palette.mode === "dark"
                  ? "linear-gradient(90deg, #60a5fa 0%, #38bdf8 100%)"
                  : "linear-gradient(90deg, #1d4ed8 0%, #0284c7 100%)",
              WebkitBackgroundClip: "text",
              WebkitTextFillColor: "transparent",
              backgroundClip: "text",
              color: "transparent",
            })}
          >
            {t("cards.welcomeTitle")}
          </Typography>
          <Typography variant="body2" color="text.secondary" sx={{ mt: 1 }}>
            {t("cards.welcomeSubtitle")}
          </Typography>
        </Box>

        <Box sx={{ display: "flex", flexWrap: "wrap", gap: 2 }}>
          <AppCard
            logo="images/bdmaster.png"
            title="BDMaster"
            intro={t("cards.introBDMaster")}
            githubUrl={BDMASTER_URL}
            isPrimary
          />
          <AppCard
            logo="images/bettermediainfo.png"
            title="BetterMediaInfo"
            intro={t("cards.introBetterMediaInfo")}
            githubUrl={BETTER_MEDIA_INFO_URL}
          />
          <AppCard
            logo="images/batchmkvextract.png"
            title="BatchMkvExtract"
            intro={t("cards.introBatchMkvExtract")}
            githubUrl={BATCH_MKV_EXTRACT_URL}
          />
        </Box>

        <Box sx={{ display: "flex", justifyContent: "center", gap: 1.5, flexWrap: "wrap" }}>
          <Button
            variant="contained"
            startIcon={<AlbumIcon />}
            onClick={() => openDiscFileDialog()}
            sx={{
              textTransform: "none",
              fontWeight: 600,
              borderRadius: 2,
              backgroundColor: "#2563eb",
              boxShadow: "0 6px 16px rgba(37,99,235,0.32)",
              "&:hover": { backgroundColor: "#1d4ed8" },
            }}
          >
            {t("cards.addDisk")}
          </Button>
          <Button
            variant="outlined"
            startIcon={<FolderIcon />}
            onClick={() => openDiscDirectoryDialog()}
            sx={{
              textTransform: "none",
              fontWeight: 600,
              borderRadius: 2,
            }}
          >
            {t("cards.addDiskFolder")}
          </Button>
        </Box>

        <Typography variant="caption" color="text.secondary" sx={{ textAlign: "center", display: "block" }}>
          {t("cards.emptyHint")}
        </Typography>

        <Box
          sx={(theme) => ({
            display: "flex",
            justifyContent: "center",
            alignItems: "center",
            gap: 2,
            flexWrap: "wrap",
            pt: 1,
            borderTop: "1px solid",
            borderColor:
              theme.palette.mode === "dark"
                ? "rgba(148,163,184,0.20)"
                : "rgba(148,163,184,0.30)",
          })}
        >
          <Box sx={{ display: "flex", alignItems: "center", gap: 0.75 }}>
            <PersonIcon fontSize="small" sx={{ color: "text.secondary" }} />
            <Typography variant="caption" color="text.secondary">
              {t("about.author")}:
            </Typography>
            <Link
              component="button"
              onClick={() => shellOpen(AUTHOR_URL)}
              underline="hover"
              sx={(theme) => ({
                fontSize: "0.75rem",
                fontWeight: 600,
                color: theme.palette.mode === "dark" ? "#93c5fd" : "#1d4ed8",
              })}
            >
              {AUTHOR_NAME}
            </Link>
          </Box>
          <Box sx={{ display: "flex", alignItems: "center", gap: 0.75 }}>
            <GitHubIcon fontSize="small" sx={{ color: "text.secondary" }} />
            <Link
              component="button"
              onClick={() => shellOpen(BDMASTER_URL)}
              underline="hover"
              sx={(theme) => ({
                fontSize: "0.75rem",
                fontWeight: 600,
                color: theme.palette.mode === "dark" ? "#93c5fd" : "#1d4ed8",
              })}
            >
              {BDMASTER_URL.replace("https://", "")}
            </Link>
          </Box>
        </Box>
      </Stack>
    </Box>
  );
}
