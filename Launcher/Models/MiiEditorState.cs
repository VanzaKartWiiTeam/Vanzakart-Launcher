namespace VanzaKartLauncher.Models;

public sealed class MiiEditorState
{
    public string Name { get; set; } = "Vanza Mii";
    public string CreatorName { get; set; } = "VanzaKart";
    public bool IsFemale { get; set; }
    public bool IsFavorite { get; set; } = true;
    public int FavoriteColorIndex { get; set; } = 4;
    public int BirthMonth { get; set; } = DateTime.Now.Month;
    public int BirthDay { get; set; } = DateTime.Now.Day;
    public int Height { get; set; } = 64;
    public int Weight { get; set; } = 64;
    public uint MiiId { get; set; }
    public byte SystemId0 { get; set; }
    public byte SystemId1 { get; set; }
    public byte SystemId2 { get; set; }
    public byte SystemId3 { get; set; }

    public int FaceShape { get; set; }
    public int SkinColor { get; set; } = 1;
    public int FacialFeature { get; set; }

    public int HairType { get; set; } = 33;
    public int HairColor { get; set; }
    public bool HairFlipped { get; set; }

    public int EyebrowType { get; set; } = 6;
    public int EyebrowRotation { get; set; } = 6;
    public int EyebrowColor { get; set; }
    public int EyebrowSize { get; set; } = 4;
    public int EyebrowVertical { get; set; } = 10;
    public int EyebrowSpacing { get; set; } = 2;

    public int EyeType { get; set; } = 2;
    public int EyeRotation { get; set; } = 4;
    public int EyeVertical { get; set; } = 12;
    public int EyeColor { get; set; }
    public int EyeSize { get; set; } = 4;
    public int EyeSpacing { get; set; } = 2;

    public int NoseType { get; set; } = 1;
    public int NoseSize { get; set; } = 4;
    public int NoseVertical { get; set; } = 9;

    public int MouthType { get; set; } = 23;
    public int MouthColor { get; set; }
    public int MouthSize { get; set; } = 4;
    public int MouthVertical { get; set; } = 13;

    public int GlassesType { get; set; }
    public int GlassesColor { get; set; }
    public int GlassesSize { get; set; } = 4;
    public int GlassesVertical { get; set; } = 10;

    public int MustacheType { get; set; }
    public int BeardType { get; set; }
    public int FacialHairColor { get; set; }
    public int MustacheSize { get; set; } = 4;
    public int MustacheVertical { get; set; } = 10;

    public bool MoleEnabled { get; set; }
    public int MoleSize { get; set; } = 4;
    public int MoleVertical { get; set; } = 10;
    public int MoleHorizontal { get; set; } = 10;

    public MiiEditorState Clone()
    {
        return (MiiEditorState)MemberwiseClone();
    }
}
