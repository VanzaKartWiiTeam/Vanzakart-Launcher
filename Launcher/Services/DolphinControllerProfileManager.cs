using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;

namespace VanzaKartLauncher.Services;

public sealed class DolphinControllerProfileManager
{
    private readonly DolphinIniService _iniService = new();

    public List<string> GetAvailableProfiles(string userFolderPath, bool isWiimote)
    {
        var result = new List<string>();
        if (string.IsNullOrWhiteSpace(userFolderPath)) return result;

        string subFolder = isWiimote ? "Wiimote" : "GCPad";
        string profilesDir = Path.Combine(userFolderPath, "Config", "Profiles", subFolder);

        if (Directory.Exists(profilesDir))
        {
            var files = Directory.GetFiles(profilesDir, "*.ini");
            foreach (var file in files)
            {
                result.Add(Path.GetFileNameWithoutExtension(file));
            }
        }

        return result;
    }

    public Dictionary<string, string> ReadActiveBindings(string userFolderPath, bool isWiimote, int portIndex = 1)
    {
        var bindings = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        if (string.IsNullOrWhiteSpace(userFolderPath)) return bindings;

        string iniFileName = isWiimote ? "WiimoteNew.ini" : "GCPadNew.ini";
        string iniPath = Path.Combine(userFolderPath, "Config", iniFileName);
        string sectionName = isWiimote ? $"Wiimote{portIndex}" : $"GCPad{portIndex}";

        if (File.Exists(iniPath))
        {
            var ini = _iniService.ReadIni(iniPath);
            if (ini.TryGetValue(sectionName, out var sectionData))
            {
                foreach (var kvp in sectionData)
                {
                    bindings[kvp.Key] = kvp.Value;
                }
            }
        }

        return bindings;
    }

    public void SaveActiveBindings(string userFolderPath, bool isWiimote, int portIndex, Dictionary<string, string> bindings)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath)) return;

        string iniFileName = isWiimote ? "WiimoteNew.ini" : "GCPadNew.ini";
        string iniPath = Path.Combine(userFolderPath, "Config", iniFileName);
        string sectionName = isWiimote ? $"Wiimote{portIndex}" : $"GCPad{portIndex}";

        var updates = new Dictionary<string, Dictionary<string, string>>
        {
            [sectionName] = bindings
        };
        _iniService.UpdateIni(iniPath, updates);
    }

    public bool SaveProfile(string userFolderPath, bool isWiimote, string profileName, Dictionary<string, string> bindings)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath) || string.IsNullOrWhiteSpace(profileName)) return false;

        string subFolder = isWiimote ? "Wiimote" : "GCPad";
        string profilesDir = Path.Combine(userFolderPath, "Config", "Profiles", subFolder);
        Directory.CreateDirectory(profilesDir);

        string profilePath = Path.Combine(profilesDir, $"{profileName}.ini");
        string sectionName = isWiimote ? "Wiimote1" : "GCPad1";

        var updates = new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase)
        {
            ["Profile"] = bindings,
            [sectionName] = bindings
        };
        _iniService.UpdateIni(profilePath, updates);
        return true;
    }

    public bool LoadProfile(string userFolderPath, bool isWiimote, int portIndex, string profileName)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath) || string.IsNullOrWhiteSpace(profileName)) return false;

        string subFolder = isWiimote ? "Wiimote" : "GCPad";
        string profilePath = Path.Combine(userFolderPath, "Config", "Profiles", subFolder, $"{profileName}.ini");

        if (!File.Exists(profilePath)) return false;

        var profileIni = _iniService.ReadIni(profilePath);
        var sourceSection = profileIni.Values.FirstOrDefault(v => v != null && v.Count > 0);
        if (sourceSection == null || sourceSection.Count == 0) return false;

        SaveActiveBindings(userFolderPath, isWiimote, portIndex, sourceSection);
        return true;
    }

    public Dictionary<string, string> ReadProfile(string userFolderPath, bool isWiimote, string profileName)
    {
        var result = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
        if (string.IsNullOrWhiteSpace(userFolderPath) || string.IsNullOrWhiteSpace(profileName)) return result;

        string subFolder = isWiimote ? "Wiimote" : "GCPad";
        string profilePath = Path.Combine(userFolderPath, "Config", "Profiles", subFolder, $"{profileName}.ini");
        if (!File.Exists(profilePath)) return result;

        var profileIni = _iniService.ReadIni(profilePath);
        var source = profileIni.TryGetValue("Profile", out var profile)
            ? profile
            : profileIni.Values.FirstOrDefault(v => v.Count > 0);
        if (source is null) return result;

        foreach (var item in source) result[item.Key] = item.Value;
        return result;
    }

    public bool DeleteProfile(string userFolderPath, bool isWiimote, string profileName)
    {
        if (string.IsNullOrWhiteSpace(userFolderPath) || string.IsNullOrWhiteSpace(profileName)) return false;

        string subFolder = isWiimote ? "Wiimote" : "GCPad";
        string profilePath = Path.Combine(userFolderPath, "Config", "Profiles", subFolder, $"{profileName}.ini");

        if (File.Exists(profilePath))
        {
            File.Delete(profilePath);
            return true;
        }

        return false;
    }
}
