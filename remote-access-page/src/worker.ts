import { Client } from "wg-webclient";

declare const self: ServiceWorkerGlobalScope;


const secret = "UDz0p8kY+v7iLwCvQZLdJCz0QgQ0ORnx5Q6bLW5Gflw=";
const peer = "M3XrOeZy6GawK650at4A9wokxp1Oy9pWIilWx2Q+MnE=";
const url = "wss://" + self.location.hostname + ":8081"
const wgClient = new Client(secret, peer, url);
