
var client;

self.addEventListener("fetch", async function(event) {
    console.log(event);
    let url = event.request.url.replace("http://localhost", "");
    console.log("fetch event", url);
    if (client && url.startsWith("/wg/")) {
        url = url.replace("/wg", "");
        const promise = new Promise(async (resolve, reject) => {
            // let body;
            // try {
            //     const text = await event.request.text();
            //     const enc = new TextEncoder();
            //     body = enc.encode(text);
            // } catch {
            //     body = undefined;
            // }
            // console.log(body);
            let response = await client.fetch(event.request, url);
            resolve(response);
        });
        event.respondWith(promise);
    }
});


self.addEventListener("activate", function() {
    self.clients.claim();
    self.clients.matchAll({type: "all", includeUncontrolled: true}).then((clients) => {
        console.log("clients:", clients);
        for (let client of clients) {
            client.postMessage("ready");
        }
    });
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
                mainThread.postMessage({
                    type: 0,
                    data: msg
                });
            });
            break;

        case "close":
            client.free();
            break;

        case "download":
            triggerDownload();
            break;
    }
});

function triggerDownload() {
    console.log("start download");
    const msg = client.get_pcap_log();
    mainThread.postMessage({
        type: 1,
        data: msg
    });
}
