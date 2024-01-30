
var client;

self.addEventListener("fetch", async function(event) {
    let url = event.request.url.replace("http://localhost", "");
    console.log("fetch event", url);
    if (client && url.startsWith("/wg/")) {
        url = url.replace("/wg", "");
        const promise = new Promise(async (resolve, reject) => {
            let body;
            try {
                const text = await event.request.text();
                const enc = new TextEncoder();
                body = enc.encode(text);
            } catch {
                body = undefined;
            }
            console.log(body);
            let response = await client.fetch(url, event.request.method, body);
            resolve(response);
        });
        event.respondWith(promise);
    }
});

var mainThread;

self.addEventListener("message", (e) => {
    mainThread = e.source;
    switch (e.data) {
        case "connect":
            console.log("connecting");
            client.disconnect_ws();
            client.start_ws();
            client.on_message(async function(msg) {
                mainThread.postMessage(msg);
                // console.log(self);
            });
            break;
    }
});
