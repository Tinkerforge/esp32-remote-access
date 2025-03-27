/* tslint:disable */
/* eslint-disable */
/**
* @param {boolean} enabled
*/
export function set_pcap_logging(enabled: boolean): void;
/**
* The exported client struct. It Wraps the actual Client and a Queue to keep the needed
* callbacks alive.
* Most function calls are simply passed to the wrapped object.
*/
export class Client {
  free(): void;
/**
* Creates a new Client struct by also creating the wrapped objects.
* @param {string} secret_str
* @param {string} peer_str
* @param {string} psk
* @param {string} url
* @param {string} internal_ip
* @param {string} internap_peer_ip
* @param {number} port
* @param {Function} disconnect_cb
* @param {Function} connect_cb
*/
  constructor(secret_str: string, peer_str: string, psk: string, url: string, internal_ip: string, internap_peer_ip: string, port: number, disconnect_cb: Function, connect_cb: Function);
/**
* Makes a http request to the provided url and return a Promise that resolves to a JS Response object.
* Internally it calls the fetch function of the wrapped WgClient object and
* registers an EventListener for the event returned by it.
* @param {Request} request
* @param {string} url
* @param {string | undefined} [username]
* @param {string | undefined} [password]
* @returns {Promise<Response>}
*/
  fetch(request: Request, url: string, username?: string, password?: string): Promise<Response>;
/**
* @param {Function} cb
*/
  start_inner_ws(cb: Function): void;
/**
*/
  disconnect_inner_ws(): void;
/**
*/
  download_pcap_log(): void;
/**
* @returns {Uint8Array}
*/
  get_pcap_log(): Uint8Array;
}
