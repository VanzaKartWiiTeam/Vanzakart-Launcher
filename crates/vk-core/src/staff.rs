//! Stemmi dello staff di VanzaKart.
//!
//! Non sono gradi: non si guadagnano, non sostituiscono il grado di prestigio
//! e chi li ha entrambi li mostra tutti e due, prima lo stemma. Il server non
//! li manda: il gioco li riconosce da solo confrontando il profile ID del
//! giocatore con una tabella scritta a mano, così nessuno può prendersi lo
//! stemma dello staff mandando dati falsi. Il launcher fa lo stesso, con la
//! stessa tabella (§D-097).
//!
//! La tabella è la copia di `sMembers` in
//! `vk-Pulsar/PulsarEngine/Network/Rating/StaffBadge.cpp`: quando cambia lì,
//! va cambiata qui.

/// Ruolo di un membro dello staff, nell'ordine dell'enum `Role` del gioco.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StaffRole {
    Moderator,
    Leader,
    StaffGhost,
    Developer,
    CreativeDirector,
    Translator,
}

impl StaffRole {
    /// Nome stabile del ruolo, quello che arriva alla UI.
    pub fn id(self) -> &'static str {
        match self {
            Self::Moderator => "moderator",
            Self::Leader => "leader",
            Self::StaffGhost => "staff_ghost",
            Self::Developer => "developer",
            Self::CreativeDirector => "creative_director",
            Self::Translator => "translator",
        }
    }

    /// Il file dello stemma in `FOOTAGE/ranks/` del sito.
    pub fn image_file(self) -> &'static str {
        match self {
            Self::Moderator => "moderator_full_00009.png",
            Self::Leader => "leader_full_00012.png",
            Self::StaffGhost => "staff_ghost_full_00010.png",
            Self::Developer => "developer_full_00000.png",
            Self::CreativeDirector => "creative_director_full_00011.png",
            Self::Translator => "translator_full_00013.png",
        }
    }
}

/// I membri dello staff: friend code senza trattini, come nel gioco.
const MEMBERS: [(u64, StaffRole); 15] = [
    (426_201_762_344, StaffRole::Leader), // 4262-0176-2344, il creatore
    (164_208_757_382, StaffRole::Leader),
    (20_202, StaffRole::CreativeDirector),
    (1_000_000_065, StaffRole::CreativeDirector),
    (500_000_060, StaffRole::CreativeDirector),
    (400_020_002, StaffRole::CreativeDirector),
    (6_010, StaffRole::CreativeDirector),
    (542_165_879_414, StaffRole::Moderator),
    (417_611_827_933, StaffRole::Moderator),
    (31_064_771_154, StaffRole::Moderator),
    (10_010_100, StaffRole::Developer),
    (267_287_972_439, StaffRole::Developer),
    (443_381_631_713, StaffRole::Developer),
    (271_582_939_761, StaffRole::StaffGhost),
    (305_544_760_230, StaffRole::StaffGhost),
];

/// Il ruolo di un friend code, comunque sia scritto (con o senza trattini).
///
/// Come `GetForProfileId` del gioco confronta il profile ID, cioè i 32 bit
/// bassi del friend code: gli altri 32 sono un checksum, e sono il profile ID
/// che il server autentica.
pub fn role_of(friend_code: &str) -> Option<StaffRole> {
    let digits: String = friend_code.chars().filter(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 12 {
        return None;
    }
    let profile_id = profile_id(digits.parse().ok()?);
    if profile_id == 0 {
        return None;
    }
    MEMBERS
        .iter()
        .find(|(code, _)| self::profile_id(*code) == profile_id)
        .map(|(_, role)| *role)
}

fn profile_id(friend_code: u64) -> u32 {
    (friend_code & 0xFFFF_FFFF) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_member_is_found_in_every_spelling() {
        assert_eq!(role_of("4262-0176-2344"), Some(StaffRole::Leader));
        assert_eq!(role_of("426201762344"), Some(StaffRole::Leader));
        assert_eq!(role_of(" 4262 0176 2344 "), Some(StaffRole::Leader));
    }

    /// Il server scrive gli zeri iniziali, la tabella del gioco no.
    #[test]
    fn leading_zeros_do_not_matter() {
        assert_eq!(role_of("0000-0002-0202"), Some(StaffRole::CreativeDirector));
        assert_eq!(role_of("0310-6477-1154"), Some(StaffRole::Moderator));
        assert_eq!(role_of("0000-1001-0100"), Some(StaffRole::Developer));
    }

    /// Stesso profile ID, checksum diverso: per il gioco è la stessa persona.
    #[test]
    fn only_the_profile_id_counts() {
        let code = 305_544_760_230_u64;
        let other_checksum = (code & 0xFFFF_FFFF) | (0x3F << 32);
        assert_eq!(
            role_of(&other_checksum.to_string()),
            Some(StaffRole::StaffGhost)
        );
    }

    #[test]
    fn everyone_else_has_no_role() {
        assert_eq!(role_of("1234-5678-9012"), None);
        assert_eq!(role_of("0000-0000-0000"), None);
        assert_eq!(role_of(""), None);
        assert_eq!(role_of("not a code"), None);
        assert_eq!(role_of("4262-0176-2344-0000"), None);
    }

    #[test]
    fn every_member_has_a_role_and_no_profile_id_repeats() {
        for (index, (code, _)) in MEMBERS.iter().enumerate() {
            assert_ne!(profile_id(*code), 0, "{code}");
            assert!(
                MEMBERS[index + 1..]
                    .iter()
                    .all(|(other, _)| profile_id(*other) != profile_id(*code)),
                "{code} ripetuto"
            );
        }
    }

    #[test]
    fn ids_and_files_are_distinct() {
        let roles = [
            StaffRole::Moderator,
            StaffRole::Leader,
            StaffRole::StaffGhost,
            StaffRole::Developer,
            StaffRole::CreativeDirector,
            StaffRole::Translator,
        ];
        for (index, role) in roles.iter().enumerate() {
            for other in &roles[index + 1..] {
                assert_ne!(role.id(), other.id());
                assert_ne!(role.image_file(), other.image_file());
            }
        }
    }
}
