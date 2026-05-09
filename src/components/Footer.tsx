/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { Box, Link, Typography } from "@mui/material";
import { open } from "@tauri-apps/plugin-shell";
import { useTranslation } from "react-i18next";

export default function Footer() {
  const { t } = useTranslation();
  return (
    <Box sx={{ my: 1.5, textAlign: "center", color: "text.secondary" }}>
      <Typography variant="caption" component="div">
        <Link
          component="button"
          onClick={() => open("https://paypal.me/caoccao?locale.x=en_US")}
        >
          {t("footer.donate")}
        </Link>
      </Typography>
      <Typography variant="caption" component="div" sx={{ mt: 0.5 }}>
        {t("footer.copyright")}{" "}
        <Link component="button" onClick={() => open("https://github.com/caoccao")}>
          Sam Cao
        </Link>{" "}
        <Link component="button" onClick={() => open("https://www.caoccao.com/")}>
          caoccao.com
        </Link>
      </Typography>
    </Box>
  );
}
