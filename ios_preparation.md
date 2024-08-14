# How to preparate the iOS-App

The iOS-App is special since it needs to be build manually and requires patches to a codebase that is not owned and maintained by us.

These changes are:

1. Adding the com.apple.developer.web-browser entitlement to the Project.entitlement file
2. Adding the WKAppBoundDomains Array property with my.warp-charger.com and mystaging.warp-charger.com entries to the MedianIOS-Info.plist file
3. Search ALL occurences of WKWebViewConfiguration objects and set their limtisNavigationsToAppBoundDomains to YES

This is needed since otherwiese iOS will block the app from creating a ServiceWorker which is essential for running the remote access.
