namespace VanzaKartLauncher.Models;

public enum MiiAvatarRenderState
{
    Unknown,
    Queued,
    Rendering,
    Ready,
    InvalidMiiData,
    Failed,
    TimedOut,
    NetworkUnavailable,
    Cancelled
}

public sealed record MiiAvatarRenderResult(
    MiiAvatarRenderState State,
    string AvatarPath,
    string Message,
    int Attempts,
    DateTime UpdatedUtc)
{
    public bool IsReady => State == MiiAvatarRenderState.Ready && !string.IsNullOrWhiteSpace(AvatarPath);

    public static MiiAvatarRenderResult Ready(string avatarPath, int attempts, string message = "Rendered")
    {
        return new MiiAvatarRenderResult(MiiAvatarRenderState.Ready, avatarPath, message, attempts, DateTime.UtcNow);
    }

    public static MiiAvatarRenderResult FromState(MiiAvatarRenderState state, string message, int attempts = 0)
    {
        return new MiiAvatarRenderResult(state, string.Empty, message, attempts, DateTime.UtcNow);
    }
}
