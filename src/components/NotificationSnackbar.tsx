/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import { Alert, Snackbar } from "@mui/material";
import * as Protocol from "../lib/protocol";
import { useAppStore } from "../lib/store";

export default function NotificationSnackbar() {
  const notification = useAppStore((state) => state.dialogNotification);
  const setNotification = useAppStore((state) => state.setDialogNotification);
  const severity = notification?.type === Protocol.DialogNotificationType.Error ? "error" : "success";
  return (
    <Snackbar
      open={notification !== null}
      autoHideDuration={5000}
      onClose={() => setNotification(null)}
      anchorOrigin={{ vertical: "top", horizontal: "center" }}
    >
      <Alert onClose={() => setNotification(null)} severity={severity} variant="filled">
        {notification?.title}
      </Alert>
    </Snackbar>
  );
}
