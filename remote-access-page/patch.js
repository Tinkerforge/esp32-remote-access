
var client;

self.addEventListener("fetch", async function(event) {
    let url = event.request.url.replace("http://localhost", "");
    console.log("fetch event", url);
    if (client && url == "/evse/state") {
        let response = client.fetch(url, "GET")
        event.respondWith(response);
    }
});
