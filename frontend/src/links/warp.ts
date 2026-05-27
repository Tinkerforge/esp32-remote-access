export const privacy_notice = "https://www.tinkerforge.com/de/home/privacy_notice";
export const terms_of_use = "https://www.tinkerforge.com/de/home/terms_and_conditions";
export const imprint = "https://www.tinkerforge.com/de/home/legal_info";
export const docs = "https://docs.warp-charger.com/";
export const app_store_id = "6736695801";
export const play_store_link = "https://play.google.com/store/apps/details?id=com.tinkerforge.warp";

export function injectAppMetaTag() {
  const meta: HTMLMetaElement = document.createElement("meta");
  meta.name = "apple-itunes-app";
  meta.content = "app-id=6736695801";
  document.getElementsByTagName("head")[0].appendChild(meta);
}
