import i18n from "i18next";
import Backend from "i18next-http-backend";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";
import { en } from "./locales/en";
import { de } from "./locales/de";

i18n
    .use(Backend)
    .use(LanguageDetector)
    .use(initReactI18next)
    .init({
        fallbackLng: "en",
        load: "currentOnly",
        interpolation: {
            escapeValue: false,
        },
        resources: {
            "en": {
                translation: en
            },
            "en-US": {
                translation: en
            },
            "de": {
                translation: de
            },
            "de-DE": {
                translation: de
            },
        },
        react: {
            defaultTransParent: "div",
        },
    });

export default i18n;
