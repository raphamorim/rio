---
layout: docs
class: docs
title: 'Install'
language: 'en'
---

## Install

You can download Rio terminal application for macOS platform, although is not stable and lacking features since the development of a beta release version is still in process.

New versions are created by weekly and monthly basis, so if you are using an unstable version make sure to keep updated. Latest canary version:

- [Download macOS - canary-781302b00980c6c5f5b04f988c3ed90709ebe99a (unstable)](https://github.com/raphamorim/rio/releases/download/canary-781302b00980c6c5f5b04f988c3ed90709ebe99a/macos-rio.zip)

Rio application is not notarized yet, so in case runs into any problem when opening it:

{% highlight toml %}
xattr -d com.apple.quarantine <path-to-rio-app>
{% endhighlight %}

<!-- ## Building from the source -->

<!-- https://github.com/raphamorim/rio/releases -->