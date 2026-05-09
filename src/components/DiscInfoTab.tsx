/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { Box, CircularProgress, Typography } from "@mui/material";
import { useTranslation } from "react-i18next";
import { useAppStore } from "../lib/store";
import DiscDetail from "./DiscDetail";
import Welcome from "./Welcome";

export default function DiscInfoTab() {
  const { t } = useTranslation();
  const disc = useAppStore((s) => s.disc);
  const scanningPath = useAppStore((s) => s.scanningPath);

  if (scanningPath) {
    return (
      <Box
        sx={{
          display: "flex",
          flexDirection: "column",
          alignItems: "center",
          justifyContent: "center",
          gap: 2,
          flex: 1,
          minHeight: 0,
          p: 4,
        }}
      >
        <CircularProgress />
        <Typography variant="body2" color="text.secondary">
          {t("cards.scanning", { path: scanningPath })}
        </Typography>
      </Box>
    );
  }

  if (!disc) {
    return <Welcome />;
  }

  return <DiscDetail />;
}
