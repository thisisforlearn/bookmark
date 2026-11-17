# SPDX-License-Identifier: GPL-3.0-only
# Copyright (C) 2026 Vaibhav
#
# Keep org.json (used by the API client) and our activities.
-keep class ai.bookmark.app.** { *; }
-keepclassmembers class * { public <init>(...); }
-dontwarn org.json.**
