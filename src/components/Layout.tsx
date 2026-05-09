/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { Box } from "@mui/material";
import Toolbar from "./Toolbar";
import MainContent from "./MainContent";
import Footer from "./Footer";

export default function Layout() {
  return (
    <Box sx={{ display: "grid", px: 1, height: "100vh", overflow: "hidden", gridTemplateRows: "auto 1fr auto" }}>
      <Box
        component="nav"
        sx={{
          flexShrink: 0,
          zIndex: 1000,
          backgroundColor: "background.default",
        }}
      >
        <Toolbar />
      </Box>
      <Box component="main" sx={{ overflow: "hidden", minHeight: 0 }}>
        <MainContent />
      </Box>
      <footer>
        <Footer />
      </footer>
    </Box>
  );
}
