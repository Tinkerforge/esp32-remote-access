import { Client } from "wg-webclient";
import { MessageType } from "./types";

declare const self: ServiceWorkerGlobalScope;


const secret = "+ATK0a+6eX1w4cZ/ueDLfJGa/er8VfCfnon/9I7Hd2s=";
const peer = "Ev900s9ZPaBFYR0qQqmv4n2zYzOH69XPqsISPf3GXD4=";
const wgClient = new Client(secret, peer, "ws://localhost:8081");

// self.addEventListener("activate", function() {
//     console.log('activate event, ha');
//     self.clients.claim();
// });

// self.addEventListener("install", function() {
//     console.log('install event');
// });

// self.addEventListener('fetch', function(event) {
//     let url = event.request.url;
//     console.log('fetch event', url);
// });

// self.setTimeout(() => {
//     let log = wgClient.get_pcap_log();
//     let url = URL.createObjectURL(new Blob([log]));
//     triggerDownload(url);
// }, 5000);

function triggerDownload(data) {
    console.log("triggerDownload");
    const msg = {
        type: MessageType.FileDownload,
        data: data
    };
    postMessage(msg);
}
