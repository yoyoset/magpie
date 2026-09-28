// The pet: an abstract, round "dumpling" bird. It keeps only the magpie's
// palette (black back, white belly, a flash of blue) so it reads as ours
// without resembling any existing character. Pure SVG + CSS, no assets.

export type Mood = "idle" | "alert" | "sleep";

export function Magpie({ mood }: { mood: Mood }) {
  const asleep = mood === "sleep";
  return (
    <svg className={`magpie mood-${mood}`} viewBox="0 0 160 160" aria-label="Magpie">
      <defs>
        <clipPath id="magpie-body">
          <circle cx="82" cy="94" r="46" />
        </clipPath>
      </defs>

      <g className="tail">
        <path d="M46 104 Q20 110 4 136 Q10 140 16 136 Q34 126 52 116 Z" fill="#1b2230" />
        <path d="M40 110 Q22 118 10 134" stroke="#3e8cdc" strokeWidth="4" strokeLinecap="round" fill="none" />
      </g>

      <g className="body">
        <circle cx="82" cy="94" r="46" fill="#1b2230" />
        <ellipse cx="86" cy="122" rx="40" ry="30" fill="#f5f7fa" clipPath="url(#magpie-body)" />
        <ellipse cx="46" cy="98" rx="11" ry="16" fill="#3e8cdc" opacity=".9" transform="rotate(18 46 98)" />
        <path d="M80 48 Q86 36 94 42" stroke="#1b2230" strokeWidth="5" strokeLinecap="round" fill="none" />
        <ellipse cx="72" cy="141" rx="5.5" ry="3" fill="#3a3f47" />
        <ellipse cx="94" cy="141" rx="5.5" ry="3" fill="#3a3f47" />
      </g>

      <g className="face">
        <ellipse cx="62" cy="92" rx="7" ry="4.5" fill="#ff8fa3" opacity=".55" />
        <ellipse cx="108" cy="92" rx="7" ry="4.5" fill="#ff8fa3" opacity=".55" />
        {asleep ? (
          <>
            <path d="M65 80 Q71 85 77 80" stroke="#f5f7fa" strokeWidth="3" fill="none" strokeLinecap="round" />
            <path d="M91 80 Q97 85 103 80" stroke="#f5f7fa" strokeWidth="3" fill="none" strokeLinecap="round" />
          </>
        ) : (
          <>
            <circle cx="71" cy="80" r="8" fill="#f5f7fa" />
            <circle cx="97" cy="80" r="8" fill="#f5f7fa" />
            <circle className="pupil" cx="72.5" cy="81" r="4.5" fill="#0b0f14" />
            <circle className="pupil" cx="98.5" cy="81" r="4.5" fill="#0b0f14" />
            <circle cx="74" cy="79" r="1.6" fill="#fff" />
            <circle cx="100" cy="79" r="1.6" fill="#fff" />
          </>
        )}
        <path d="M80 90 L88 90 L84 96 Z" fill="#f5c04a" strokeLinejoin="round" />
      </g>

      {mood === "alert" && (
        <text className="bang" x="120" y="44" fill="#f0a030" fontSize="24" fontWeight="800">
          !
        </text>
      )}
      {asleep && (
        <text className="zzz" x="118" y="46" fill="#8a99aa" fontSize="16" fontWeight="700">
          z
        </text>
      )}
    </svg>
  );
}
