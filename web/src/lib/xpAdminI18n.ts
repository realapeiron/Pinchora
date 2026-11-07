// English keys form the English catalog; Turkish follows the browser language.
const tr: Record<string, string> = {
  "MEMBER XP": "ÜYE XP’Sİ",
  "Find a member by name or Discord ID, then adjust their server XP.":
    "Üyeyi adıyla veya Discord kimliğiyle bulup sunucu XP’sini düzenleyin.",
  "Find member": "Üye bul",
  "Name or Discord ID": "Ad veya Discord kimliği",
  Amount: "Miktar",
  "Enter a whole number from 0 to 1,000,000,000.": "0 ile 1.000.000.000 arasında bir tam sayı girin.",
  "No members found.": "Üye bulunamadı.",
  "Member search results": "Üye arama sonuçları",
  Change: "İşlem",
  "Add XP": "XP ekle",
  "Remove XP": "XP çıkar",
  "Set XP": "XP’yi ayarla",
  "Save XP": "XP’yi kaydet",
  "Member XP saved.": "Üyenin XP’si kaydedildi.",
  "IMPORT FROM MEE6": "MEE6’DAN İÇE AKTAR",
  "Copy member levels from MEE6. You can run one import per server every 24 hours.":
    "Üye seviyelerini MEE6’dan aktarın. Her sunucuda 24 saatte bir aktarım yapabilirsiniz.",
  "When a member already has XP": "Üyenin zaten XP’si varsa",
  "Keep the higher XP": "Yüksek olan XP’yi koru",
  "Use MEE6 XP": "MEE6 XP’sini kullan",
  "Review import": "Aktarımı gözden geçir",
  "Existing XP stays if it is higher than MEE6 XP.": "Mevcut XP, MEE6 XP’sinden yüksekse korunur.",
  "Imported members will use their MEE6 XP.": "Aktarılan üyelerin MEE6 XP’si kullanılır.",
  "Confirm to start the import.": "Aktarımı başlatmak için onaylayın.",
  "Confirm import": "Aktarımı onayla",
  Cancel: "Vazgeç",
  "Checked {count} members. Import {status}.": "{count} üye kontrol edildi. Aktarım: {status}.",
  starting: "başlıyor",
  running: "sürüyor",
  done: "tamamlandı",
  failed: "başarısız",
};

export function t(english: string): string {
  return typeof navigator !== "undefined" && navigator.language.toLowerCase().startsWith("tr")
    ? (tr[english] ?? english)
    : english;
}

export function tf(english: string, values: Record<string, string | number>): string {
  return t(english).replace(/\{(\w+)\}/g, (all, key: string) =>
    Object.prototype.hasOwnProperty.call(values, key) ? String(values[key]) : all,
  );
}
