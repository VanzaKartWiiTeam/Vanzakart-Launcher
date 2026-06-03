# 🏎️ VanzaKart Launcher

[![Official Website](https://img.shields.io/badge/Official_Website-sitodaking.it-blue?style=for-the-badge)](https://sitodaking.it)

A custom-built, modern, and professional launcher designed to manage and boot VanzaKart mods. Perfectly optimized for Wii emulation through Dolphin and Riivolution patches, it gets you straight into the race with zero hassle.

## ✨ Key Features

* **Sleek Gaming Interface:** A dark-themed, edgy UI enhanced with subtle neon accents and glow gradients, delivering a premium and immersive user experience right from the desktop.
* **Smart Update System:** Built-in network and extraction engines that automatically download, unpack, and install the latest patches, ensuring your game is always up to date.
* **Streamlined Deployment:** Comes with dedicated Setup and Uninstaller modules, guaranteeing a clean, safe, and effortless installation process on Windows environments.
* **Emulation Ready:** Specifically tailored to hook seamlessly into Dolphin and manage your Riivolution setups without requiring manual folder configurations.

## 🚀 How to Download & Install

**Option 1: Official Website (Recommended)**  
You can always find the latest stable version of the mod and the launcher directly on our website:  
🌐 **[sitodaking.it](https://sitodaking.it)**

**Option 2: GitHub Releases**  
1. Grab the latest release from the **Releases** tab on this repository.
2. Run the provided Setup executable.
3. Follow the quick installation wizard.
4. Launch VanzaKart and hit the track!

## 🛠️ Technical Architecture

* **Language:** C#
* **UI Framework:** WPF (Windows Presentation Foundation)
* **Core Engine:** A modular architecture built for performance, featuring dedicated controllers (`NetworkService`, `ArchiveService`, `SettingsService`) that handle file integrity, background downloads, and configuration management without slowing down the boot sequence.

## 💻 Development & Contributing

The launcher is built with C# and WPF. To dive into the code, simply clone the repository and open the solution in Visual Studio or JetBrains Rider. 

Standard NuGet package restoration applies. Just ensure your IDE is configured for .NET desktop development, set the `Launcher` project as your startup target, and you are ready to compile. Pull requests for optimizations, bug fixes, or new features are always welcome.

---
*Built with passion for the Mario Kart modding community.*
