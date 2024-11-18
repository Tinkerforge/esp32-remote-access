/**
 * This file was auto-generated by openapi-typescript.
 * Do not make direct changes to the file.
 */

export interface paths {
    "/auth/generate_salt": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Generate random bytes */
        get: operations["generate_salt"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/get_login_salt": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Get the salt needed to derive the login-key. */
        get: operations["get_login_salt"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/jwt_refresh": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Refresh the jwt-token. A valid refresh-token is needed. */
        get: operations["jwt_refresh"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/login": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Login user */
        post: operations["login"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/recovery": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post: operations["recovery"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/register": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        /** Register a new user */
        post: operations["register"];
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/start_recovery": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Start the process of account recovery. */
        get: operations["start_recovery"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/auth/verify": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Verify a registered user. */
        get: operations["verify"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/charger/add": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Add a new charger. */
        put: operations["add"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/charger/allow_user": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Give another user permission to access a charger owned by the user. */
        put: operations["allow_user"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/charger/get_chargers": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Get all chargers that the current user has access to. */
        get: operations["get_chargers"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/charger/get_key": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_key"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/charger/remove": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["remove"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/charger/selfdestruct": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["selfdestruct"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/management": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Route for the charger to be identifiable via the ip. */
        put: operations["management"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/user/delete": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        put?: never;
        post?: never;
        delete: operations["delete_user"];
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/user/get_secret": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get: operations["get_secret"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/user/logout": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Logout user */
        get: operations["logout"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/user/me": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        /** Get information about the currently logged in user. */
        get: operations["me"];
        put?: never;
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/user/update_password": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Update the user password */
        put: operations["update_password"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
    "/user/update_user": {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        get?: never;
        /** Update basic user information. */
        put: operations["update_user"];
        post?: never;
        delete?: never;
        options?: never;
        head?: never;
        patch?: never;
        trace?: never;
    };
}
export type webhooks = Record<string, never>;
export interface components {
    schemas: {
        AddChargerResponseSchema: {
            charger_password: string;
            charger_uuid: string;
            management_pub: string;
        };
        AddChargerSchema: {
            charger: components["schemas"]["ChargerSchema"];
            keys: components["schemas"]["Keys"][];
            name: string;
            note: string;
        };
        AllowUserSchema: {
            charger_id: string;
            charger_name: number[];
            charger_password: string;
            email: string;
            note: string;
            user_auth: components["schemas"]["UserAuth"];
            wg_keys: components["schemas"]["Keys"][];
        };
        ChargerSchema: {
            charger_pub: string;
            psk: string;
            uid: string;
            wg_charger_ip: string;
            wg_server_ip: string;
        };
        /** @enum {string} */
        ChargerStatus: "Disconnected" | "Connected";
        DeleteChargerSchema: {
            charger: string;
        };
        DeleteUserSchema: {
            login_key: number[];
        };
        FilteredUser: {
            email: string;
            id: string;
            name: string;
        };
        GetChargerSchema: {
            id: string;
            name: string;
            note?: string | null;
            /** Format: int32 */
            port: number;
            status: components["schemas"]["ChargerStatus"];
            /** Format: int32 */
            uid: number;
            valid: boolean;
        };
        GetSecretResponse: {
            secret: number[];
            secret_nonce: number[];
            secret_salt: number[];
        };
        GetWgKeysResponseSchema: {
            charger_address: string;
            charger_id: string;
            charger_pub: string;
            id: string;
            psk: number[];
            web_address: string;
            web_private: number[];
        };
        Keys: {
            charger_address: string;
            charger_public: string;
            /** Format: int32 */
            connection_no: number;
            psk: number[];
            web_address: string;
            web_private: number[];
        };
        LoginSchema: {
            email: string;
            login_key: number[];
        };
        ManagementDataVersion: {
            V1: components["schemas"]["ManagementDataVersion1"];
        } | {
            V2: components["schemas"]["ManagementDataVersion2"];
        };
        ManagementDataVersion1: {
            configured_connections: number[];
            firmware_version: string;
            /** Format: int32 */
            port: number;
        };
        ManagementDataVersion2: {
            configured_connections: number[];
            firmware_version: string;
            id: string;
            password: string;
            /** Format: int32 */
            port: number;
        };
        ManagementResponseSchema: {
            configured_connections: number[];
            /** Format: int64 */
            time: number;
            uuid?: string | null;
        };
        ManagementSchema: {
            data: components["schemas"]["ManagementDataVersion"];
            /** Format: int32 */
            id?: number | null;
            password?: string | null;
        };
        PasswordUpdateSchema: {
            new_encrypted_secret: number[];
            new_login_key: number[];
            new_login_salt: number[];
            new_secret_nonce: number[];
            new_secret_salt: number[];
            old_login_key: number[];
        };
        RecoverySchema: {
            new_encrypted_secret: number[];
            new_login_key: number[];
            new_login_salt: number[];
            new_secret_nonce: number[];
            new_secret_salt: number[];
            recovery_key: string;
            reused_secret: boolean;
        };
        RegisterSchema: {
            email: string;
            login_key: number[];
            login_salt: number[];
            name: string;
            secret: number[];
            secret_nonce: number[];
            secret_salt: number[];
        };
        SelfdestructSchema: {
            /** Format: int32 */
            id?: number | null;
            password: string;
            uuid?: string | null;
        };
        UserAuth: {
            LoginKey: string;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
}
export type $defs = Record<string, never>;
export interface operations {
    generate_salt: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": number[];
                };
            };
        };
    };
    get_login_salt: {
        parameters: {
            query: {
                email: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": number[];
                };
            };
            /** @description User does not exist */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    jwt_refresh: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description The refresh token was invalid */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "text/plain": string;
                };
            };
        };
    };
    login: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["LoginSchema"];
            };
        };
        responses: {
            /** @description Login was successful */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Credentials were incorrect */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    recovery: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["RecoverySchema"];
            };
        };
        responses: {
            /** @description Recovery was successful */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Request contained invalid data */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    register: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["RegisterSchema"];
            };
        };
        responses: {
            /** @description Registration was successful */
            201: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description A user with this email already exists */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    start_recovery: {
        parameters: {
            query: {
                email: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Request was successful */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description User does not exist */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    verify: {
        parameters: {
            query: {
                /** @description Verification id that was sent to the user via email. */
                id: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Verification was successful and a redirect to the login is sent. */
            307: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description There is no verification request or the account was already verified. */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    add: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AddChargerSchema"];
            };
        };
        responses: {
            /** @description Adding or updating the charger was successful. */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["AddChargerResponseSchema"];
                };
            };
            /** @description The charger already exists with another owner */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    allow_user: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["AllowUserSchema"];
            };
        };
        responses: {
            /** @description Allowing the user to access the charger was successful. */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description The user does not exist. */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_chargers: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description Success */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetChargerSchema"][];
                };
            };
            /** @description Somehow got a valid jwt but the user does not exist. */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_key: {
        parameters: {
            query: {
                cid: string;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetWgKeysResponseSchema"];
                };
            };
            /** @description Somehow got a valid jwt but the user does not exist. */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description All keys for this charger are currently in use */
            404: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    remove: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DeleteChargerSchema"];
            };
        };
        responses: {
            /** @description Deletion was successful. */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description The user sending the request is not the owner of the charger. */
            409: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    selfdestruct: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["SelfdestructSchema"];
            };
        };
        responses: {
            /** @description Everything worked fine and the charger was deleted */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    management: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["ManagementSchema"];
            };
        };
        responses: {
            /** @description Identification was successful */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["ManagementResponseSchema"];
                };
            };
            /** @description Got no valid ip address for the charger */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description The charger id and password do not match */
            401: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    delete_user: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["DeleteUserSchema"];
            };
        };
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Wrong password */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            500: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    get_secret: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["GetSecretResponse"];
                };
            };
        };
    };
    logout: {
        parameters: {
            query: {
                logout_all: boolean;
            };
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            /** @description User logged out */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    me: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody?: never;
        responses: {
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content: {
                    "application/json": components["schemas"]["FilteredUser"];
                };
            };
            /** @description The jwt token was somehow valid but contained a non valid uuid. */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    update_password: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["PasswordUpdateSchema"];
            };
        };
        responses: {
            /** @description Password update was successful. */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
            /** @description The old password was wrong. */
            400: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
    update_user: {
        parameters: {
            query?: never;
            header?: never;
            path?: never;
            cookie?: never;
        };
        requestBody: {
            content: {
                "application/json": components["schemas"]["FilteredUser"];
            };
        };
        responses: {
            /** @description Update was successful. */
            200: {
                headers: {
                    [name: string]: unknown;
                };
                content?: never;
            };
        };
    };
}
