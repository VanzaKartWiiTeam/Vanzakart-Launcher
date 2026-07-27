using System.Net.Http;
using System.Text;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace VanzaKartLauncher.Services;

public sealed class BetaTokenVerificationResult
{
    [JsonPropertyName("success")]
    public bool Success { get; set; }

    [JsonPropertyName("message")]
    public string Message { get; set; } = string.Empty;

    [JsonPropertyName("error")]
    public string Error { get; set; } = string.Empty;

    public bool IsNetworkOrServerError { get; set; }

    public bool IsSuccess => Success;
    public string DisplayErrorMessage => !string.IsNullOrWhiteSpace(Error) ? Error : "Invalid access token.";
}

public sealed class BetaAccessService
{
    private static readonly HttpClient SharedHttpClient = new()
    {
        Timeout = TimeSpan.FromSeconds(10)
    };

    public async Task<BetaTokenVerificationResult> VerifyTokenAsync(string token, CancellationToken cancellationToken = default)
    {
        if (string.IsNullOrWhiteSpace(token))
        {
            return new BetaTokenVerificationResult
            {
                Success = false,
                Error = "Please enter an Access Token.",
                IsNetworkOrServerError = false
            };
        }

        try
        {
            var cleanToken = token.Trim();
            var jsonPayload = JsonSerializer.Serialize(new { token = cleanToken });
            using var content = new StringContent(jsonPayload, Encoding.UTF8, "application/json");

            var response = await SharedHttpClient.PostAsync(LauncherConfig.BetaTokenVerifyApiUrl, content, cancellationToken);
            var responseJson = await response.Content.ReadAsStringAsync(cancellationToken);

            var result = JsonSerializer.Deserialize<BetaTokenVerificationResult>(responseJson, new JsonSerializerOptions
            {
                PropertyNameCaseInsensitive = true
            });

            if (result != null)
            {
                result.IsNetworkOrServerError = false;
                return result;
            }

            return new BetaTokenVerificationResult
            {
                Success = false,
                Error = "Invalid response from verification server.",
                IsNetworkOrServerError = false
            };
        }
        catch (OperationCanceledException)
        {
            return new BetaTokenVerificationResult
            {
                Success = false,
                Error = "Verification request timed out. Please check your network connection.",
                IsNetworkOrServerError = true
            };
        }
        catch (HttpRequestException ex)
        {
            return new BetaTokenVerificationResult
            {
                Success = false,
                Error = $"Server error ({ex.StatusCode?.ToString() ?? "Connection failed"}). Please try again later.",
                IsNetworkOrServerError = true
            };
        }
        catch (Exception ex)
        {
            return new BetaTokenVerificationResult
            {
                Success = false,
                Error = $"Verification error: {ex.Message}",
                IsNetworkOrServerError = true
            };
        }
    }
}
