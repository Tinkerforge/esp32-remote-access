import { description } from "translations-en";

export const en = {
    "alert_default_text": "An Error occured!",
    "alert_default_success": "Success!",
    "description": description,
    "user": {
        "profile_information": "User information",
        "save_changes": "Save changes",
        "user_id": "User-ID",
        "email": "Email-address",
        "name": "Name",
        "change": "Change",
        "change_password": "Change password",
        "current_password": "Current password",
        "current_password_error_message": "Must not be empty.",
        "new_password": "New password",
        "new_password_error_message": "Must contain at least one number and one uppercase and lowercase letter, and at least 8 or more characters",
        "close": "Close",
        "delete_user": "Delete account",
        "password": "Password",
        "password_invalid": "Password is wrong",
        "logout_all": "Logout from all sessions",
        "get_user_failed": "Loading user data failed with status code {{status}}: {{response}}",
        "update_user_failed": "Updating the user-data failed with status code {{status}}: {{response}}",
        "update_password_failed": "Updating the password failed with status code {{status}}: {{response}}",
        "delete_user_failed": "Deleting user failed with status code {{status}}: {{response}}",
        "account_actions": "Account Actions",
        "email_change_disabled": "Email changes are disabled because you have chargers with old firmware versions. Please update your chargers first.",
        "local_settings": "Lokal Settings",
        "debug_mode": "Debug-Modus",
    },
    "recovery": {
        "recovery": "Password recovery",
        "new_password": "New password",
        "recovery_file": "Recovery File",
        "submit": "Submit",
        "invalid_file": "File is invalid"
    },
    "chargers": {
        "charger_name": "Name",
        "charger_id": "Device-ID",
        "mobile_charger_id": "ID",
        "status": "Status",
        "status_connected": "Connected",
        "status_disconnected": "Disconnected",
        "connect": "Connect",
        "remove": "Remove",
        "close": "Close",
        "loading_secret_failed": "Loading secret failed with status code {{status}}: {{response}}",
        "loading_keys_failed": "Loading connection keys failed with status code {{status}}: {{response}}",
        "all_keys_in_use": "Currently all remote connections are in use",
        "no_keys": "You need to register this device again.",
        "connect_error_text": "Connecting to device {{charger_id}} failed with status code {{status}}: {{response}}",
        "remove_error_text": "Removing device {{charger_id}} failed with status code {{status}}: {{response}}",
        "delete_modal_heading": "Remove device {{name}}",
        "delete_modal_body": "Are you sure you want to remove device {{name}}? This is permanently and the device can only be added again once you have direct access to the device again.",
        "select_sorting": "Sort",
        "sorting_sequence_asc": "Ascending",
        "sorting_sequence_desc": "Descending",
        "note": "Note",
        "edit_note_heading": "Edit note",
        "edit_note_failed": "Editing note failed",
        "accept": "Accept",
        "decline": "Decline",
        "show_more": "Show more",
        "show_less": "Show less"
    },
    "navbar": {
        "home": "Home",
        "user": "User",
        "chargers": "Devices",
        "logout": "Logout",
        "token": "Token",
        "close": "Schließen"
    },
    "register": {
        "name": "Name",
        "name_error_message": "The name must not be empty",
        "email": "Email-address",
        "email_error_message": "The email-address must not be empty",
        "password": "Password",
        "password_error_message": "Must contain at least one number and one uppercase and lowercase letter, and at least 8 or more characters",
        "accept_privacy_notice": "I have read, understood and I am accepting the <0>privacy notice</0>.",
        "accept_terms_and_conditions": "I have read, understood and I am accepting the <0>terms and conditions</0>.",
        "accept_privacy_notice_alpha": "I understand that this is an alpha-version and accept the usage of my data as described <0>here</0>.",
        "register": "Register",
        "save_recovery_data": "Save recovery file",
        "save": "Save",
        "save_recovery_data_text": "Since we can only decrypt the access code with the correct password we need this file to recover access to your devices in case of a password loss. Save this file in a safe location which is inaccessible by others since it is equivalent to your password.",
        "close": "Close",
        "registration_successful": "Registration was successful, you should receive an email in the next couple of minutes."
    },
    "login": {
        "password_recovery": "Password reset",
        "email":"Email-address",
        "send": "Send",
        "close": "Close",
        "password": "Password",
        "login": "Login",
        "wrong_credentials": "Email-adresse or password wrong.",
        "success_alert_text": "You should receive an email in the next couple of minutes.",
        "success_alert_heading": "Success",
        "error_alert_text": "Failed to start recovery with status {{- status}}: {{text}}",
        "verify_before_login": "Please verify your email address before logging in",
        "verify_before_login_heading": "Email not verified"
    },
    "footer": {
        "imprint": "Imprint",
        "terms_of_use": "Terms and Conditions",
        "privacy_notice": "Privacy Notice"
    },
    "app": {
        "close_remote_access": "Close Remote-Access"
    },
    tokens: {
        fetch_user_failed: "Failed to fetch user",
        fetch_tokens_failed: "Failed to fetch tokens",
        unexpected_error: "An unexpected error occurred",
        create_token_failed: "Failed to create token",
        delete_token_failed: "Failed to delete token",
        copy_success: "Success",
        copy_success_text: "Token copied to clipboard",
        copy_failed: "Failed to copy token",
        create_token: "Create authorization token",
        use_once: "Use once",
        create: "Create token",
        existing_tokens: "Existing tokens",
        reusable: "Reusable",
        copy: "Copy",
        delete: "Delete",
        single_use_description: "This token can only be used once and will automatically expire after first use",
        multi_use_description: "This token can be used multiple times until manually deleted"
    }
};
