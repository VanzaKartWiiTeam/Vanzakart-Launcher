using System;
using System.Collections.Generic;
using System.IO;
using System.Text;

namespace VanzaKartLauncher.Services;

/// <summary>
/// Safe, format-preserving INI file reader and writer for Dolphin Emulator configuration files.
/// </summary>
public sealed class DolphinIniService
{
    public Dictionary<string, Dictionary<string, string>> ReadIni(string filePath)
    {
        var result = new Dictionary<string, Dictionary<string, string>>(StringComparer.OrdinalIgnoreCase);
        if (!File.Exists(filePath))
        {
            return result;
        }

        string currentSection = "Global";
        result[currentSection] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

        foreach (var rawLine in File.ReadAllLines(filePath))
        {
            var line = rawLine.Trim();
            if (string.IsNullOrWhiteSpace(line) || line.StartsWith(";") || line.StartsWith("#"))
            {
                continue;
            }

            if (line.StartsWith("[") && line.EndsWith("]"))
            {
                currentSection = line.Substring(1, line.Length - 2).Trim();
                if (!result.ContainsKey(currentSection))
                {
                    result[currentSection] = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
                }
                continue;
            }

            int eqIndex = line.IndexOf('=');
            if (eqIndex > 0)
            {
                string key = line.Substring(0, eqIndex).Trim();
                string val = line.Substring(eqIndex + 1).Trim();
                if (!result[currentSection].ContainsKey(key))
                {
                    result[currentSection][key] = val;
                }
            }
        }

        return result;
    }

    public void UpdateIni(string filePath, Dictionary<string, Dictionary<string, string>> updates)
    {
        try
        {
            var directory = Path.GetDirectoryName(filePath);
            if (!string.IsNullOrEmpty(directory) && !Directory.Exists(directory))
            {
                Directory.CreateDirectory(directory);
            }

            var lines = File.Exists(filePath) ? new List<string>(File.ReadAllLines(filePath)) : new List<string>();
            var modifiedLines = new List<string>();
            var processedKeys = new Dictionary<string, HashSet<string>>(StringComparer.OrdinalIgnoreCase);

            foreach (var kvp in updates)
            {
                processedKeys[kvp.Key] = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
            }

            string currentSection = "Global";

            for (int i = 0; i < lines.Count; i++)
            {
                string rawLine = lines[i];
                string trimmed = rawLine.Trim();

                if (trimmed.StartsWith("[") && trimmed.EndsWith("]"))
                {
                    // If we left a section, append any remaining new keys for that section
                    if (updates.TryGetValue(currentSection, out var prevSectionUpdates))
                    {
                        foreach (var keyVal in prevSectionUpdates)
                        {
                            if (!processedKeys[currentSection].Contains(keyVal.Key))
                            {
                                modifiedLines.Add($"{keyVal.Key} = {keyVal.Value}");
                                processedKeys[currentSection].Add(keyVal.Key);
                            }
                        }
                    }

                    currentSection = trimmed.Substring(1, trimmed.Length - 2).Trim();
                    modifiedLines.Add(rawLine);
                    continue;
                }

                int eqIndex = trimmed.IndexOf('=');
                if (eqIndex > 0 && updates.TryGetValue(currentSection, out var sectionUpdates))
                {
                    string key = trimmed.Substring(0, eqIndex).Trim();
                    if (sectionUpdates.TryGetValue(key, out string? newValue))
                    {
                        modifiedLines.Add($"{key} = {newValue}");
                        processedKeys[currentSection].Add(key);
                        continue;
                    }
                }

                modifiedLines.Add(rawLine);
            }

            // Flush remaining keys for the last section processed in the file loop
            if (updates.TryGetValue(currentSection, out var lastSectionUpdates))
            {
                foreach (var keyVal in lastSectionUpdates)
                {
                    if (!processedKeys[currentSection].Contains(keyVal.Key))
                    {
                        modifiedLines.Add($"{keyVal.Key} = {keyVal.Value}");
                        processedKeys[currentSection].Add(keyVal.Key);
                    }
                }
            }

            // Append completely new sections that didn't exist in the file
            foreach (var sectionKvp in updates)
            {
                string sectionName = sectionKvp.Key;
                if (!processedKeys.TryGetValue(sectionName, out var set) || set.Count < sectionKvp.Value.Count)
                {
                    if (!lines.Any(l => l.Trim().Equals($"[{sectionName}]", StringComparison.OrdinalIgnoreCase)))
                    {
                        if (modifiedLines.Count > 0 && !string.IsNullOrWhiteSpace(modifiedLines[^1]))
                        {
                            modifiedLines.Add(string.Empty);
                        }
                        modifiedLines.Add($"[{sectionName}]");
                        foreach (var keyVal in sectionKvp.Value)
                        {
                            if (set == null || !set.Contains(keyVal.Key))
                            {
                                modifiedLines.Add($"{keyVal.Key} = {keyVal.Value}");
                            }
                        }
                    }
                }
            }

            // Write safely
            string tempFile = filePath + ".tmp";
            File.WriteAllLines(tempFile, modifiedLines, Encoding.UTF8);
            File.Move(tempFile, filePath, overwrite: true);
        }
        catch (Exception ex)
        {
            System.Diagnostics.Debug.WriteLine($"[DolphinIniService] Error updating {filePath}: {ex.Message}");
        }
    }
}
