/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import enUS from "./locales/en-US.json";
import zhCN from "./locales/zh-CN.json";

i18n.use(initReactI18next).init({
  resources: {
    "en-US": { translation: enUS },
    "zh-CN": { translation: zhCN },
  },
  lng: "en-US",
  fallbackLng: "en-US",
  interpolation: {
    escapeValue: false,
  },
});

export function changeLanguage(language: string) {
  i18n.changeLanguage(language);
}

export default i18n;
