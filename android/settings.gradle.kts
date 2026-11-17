// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Vaibhav

pluginManagement {
    repositories {
        google()
        // Mirror of Maven Central; the canonical host rate-limits CI IPs (429).
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        mavenCentral()
    }
}

rootProject.name = "bookmark"
include(":app")
