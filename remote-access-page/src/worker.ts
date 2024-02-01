import { Client } from "wg-webclient";

declare const self: ServiceWorkerGlobalScope;


const secret = "+ATK0a+6eX1w4cZ/ueDLfJGa/er8VfCfnon/9I7Hd2s=";
const peer = "Ev900s9ZPaBFYR0qQqmv4n2zYzOH69XPqsISPf3GXD4=";
const wgClient = new Client(secret, peer, "ws://localhost:8081");
