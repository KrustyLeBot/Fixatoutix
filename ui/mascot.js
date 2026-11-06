// The village blacksmith: an original character (stocky, braided moustache, leather apron, hammer).
// Moods: "guard" (watchful, hammer up), "banquet" (asleep at the table), "angry" (a window resists).
window.mascot = function (mood) {
  const skin = "#F0C79C", hair = "#B4521B", hairDark = "#7E3510", leather = "#8A5A3A", steel = "#6B7079";
  const eyes =
    mood === "banquet"
      ? `<path d="M46 50q4 4 8 0M66 50q4 4 8 0" fill="none" stroke="#2B2118" stroke-width="2.4" stroke-linecap="round"/>`
      : mood === "angry"
      ? `<circle cx="50" cy="51" r="2.6" fill="#2B2118"/><circle cx="70" cy="51" r="2.6" fill="#2B2118"/>
         <path d="M43 43l12 4M77 43l-12 4" stroke="#2B2118" stroke-width="3" stroke-linecap="round"/>`
      : `<circle cx="50" cy="51" r="2.6" fill="#2B2118"/><circle cx="70" cy="51" r="2.6" fill="#2B2118"/>
         <path d="M44 45l10-2M76 45l-10-2" stroke="#2B2118" stroke-width="2.6" stroke-linecap="round"/>`;
  const extra =
    mood === "banquet"
      ? `<path d="M86 22h9l-9 9h9M98 10h7l-7 7h7" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round" opacity=".6"/>
         <rect x="14" y="96" width="92" height="10" rx="3" fill="${leather}"/>
         <ellipse cx="86" cy="94" rx="14" ry="4" fill="#d9c9b4"/><path d="M76 92q10-12 20 0z" fill="#c07a3a"/>`
      : mood === "angry"
      ? `<path d="M92 34q4 6 0 10q-4-4 0-10z" fill="#6cb4e4" opacity=".85"/>`
      : "";
  const hammer =
    mood === "banquet"
      ? ""
      : `<g transform="rotate(${mood === "angry" ? 10 : 24} 94 70)">
           <rect x="91" y="22" width="6" height="50" rx="3" fill="#8B5E3C"/>
           <rect x="78" y="14" width="30" height="14" rx="3" fill="${steel}"/>
           <rect x="78" y="14" width="30" height="5" rx="2" fill="#8C919B"/>
         </g>
         <circle cx="94" cy="76" r="7" fill="${skin}"/>`;
  return `<svg class="mascot" viewBox="0 0 130 120" role="img" aria-label="Fixatoutix" xmlns="http://www.w3.org/2000/svg">
    <path d="M26 118Q26 82 60 80Q94 82 94 118z" fill="${leather}"/>
    <path d="M42 118V92h36v26z" fill="#6D4328" opacity=".55"/>
    <path d="M44 82h32l-4 8H48z" fill="#E8DCCB"/>
    ${hammer}
    <circle cx="60" cy="52" r="25" fill="${skin}"/>
    <path d="M35 42L29 24l15 8 4-15 9 13 9-15 4 17 15-8-5 20Q60 28 35 42z" fill="${hair}"/>
    ${eyes}
    <circle cx="60" cy="58" r="4.5" fill="#E2A982"/>
    <path d="M60 62Q47 56 38 64Q32 72 38 88q5-6 5-14 8-5 17-6z" fill="${hair}"/>
    <path d="M60 62Q73 56 82 64Q88 72 82 88q-5-6-5-14-8-5-17-6z" fill="${hair}"/>
    <path d="M38 88q-2 8 3 12M82 88q2 8-3 12" fill="none" stroke="${hairDark}" stroke-width="3" stroke-linecap="round"/>
    <circle cx="41" cy="101" r="3" fill="${hairDark}"/><circle cx="79" cy="101" r="3" fill="${hairDark}"/>
    <path d="M44 70q8 3 16 0M60 70q8 3 16 0" fill="none" stroke="${hairDark}" stroke-width="1.6" stroke-linecap="round" opacity=".7"/>
    ${extra}
  </svg>`;
};
