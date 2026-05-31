/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.

 *   Licensed under the Apache License, Version 2.0 (the "License");
 *   you may not use this file except in compliance with the License.
 *   You may obtain a copy of the License at

 *   http://www.apache.org/licenses/LICENSE-2.0

 *   Unless required by applicable law or agreed to in writing, software
 *   distributed under the License is distributed on an "AS IS" BASIS,
 *   WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 *   See the License for the specific language governing permissions and
 *   limitations under the License.
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
