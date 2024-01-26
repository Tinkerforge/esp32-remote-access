/// <reference lib="webworker" />

import { Client } from "wg-webclient";

declare const self: ServiceWorkerGlobalScope;


const secret = "+ATK0a+6eX1w4cZ/ueDLfJGa/er8VfCfnon/9I7Hd2s=";
const peer = "Ev900s9ZPaBFYR0qQqmv4n2zYzOH69XPqsISPf3GXD4=";
const wgClient = new Client(secret, peer, "ws://localhost:8081");

self.addEventListener("activate", function() {
    self.performance.now();
    console.log('activate event');
});

self.addEventListener("install", function() {
    console.log('install event');
    self.skipWaiting();
});

self.addEventListener('fetch', function(event) {
    let url = event.request.url;
    console.log('fetch event', url);
});
