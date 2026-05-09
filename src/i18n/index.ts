/*
 *   Copyright (c) 2026. caoccao.com Sam Cao
 *   All rights reserved.
 */

import i18n from "i18next";
import { initReactI18next } from "react-i18next";
import de from "./locales/de.json";
import enUS from "./locales/en-US.json";
import es from "./locales/es.json";
import fr from "./locales/fr.json";
import ja from "./locales/ja.json";
import zhCN from "./locales/zh-CN.json";
import zhHK from "./locales/zh-HK.json";
import zhTW from "./locales/zh-TW.json";

i18n.use(initReactI18next).init({
  resources: {
    "de": { translation: de },
    "en-US": { translation: enUS },
    "es": { translation: es },
    "fr": { translation: fr },
    "ja": { translation: ja },
    "zh-CN": { translation: zhCN },
    "zh-HK": { translation: zhHK },
    "zh-TW": { translation: zhTW },
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
