function Invoke-HermesModelDownloadTransfer {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] $Context,
        [Parameter(Mandatory)] $File,
        [Parameter(Mandatory)][long] $CompletedBefore,
        [Nullable[long]] $OverallTotal
    )

    [System.IO.Directory]::CreateDirectory([System.IO.Path]::GetDirectoryName($File.partialPath)) | Out-Null
    $existing = $(if (Test-Path -LiteralPath $File.partialPath -PathType Leaf) {
        (Get-Item -LiteralPath $File.partialPath).Length
    } else { 0L })
    if ($File.expectedSizeBytes -and $existing -gt [long]$File.expectedSizeBytes) {
        throw "Partial file for '$($File.filename)' is larger than the declared source artifact."
    }

    $handler = [System.Net.Http.HttpClientHandler]::new()
    $handler.AllowAutoRedirect = $true
    $handler.MaxAutomaticRedirections = 5
    $client = [System.Net.Http.HttpClient]::new($handler)
    $client.Timeout = [TimeSpan]::FromHours(24)
    $request = [System.Net.Http.HttpRequestMessage]::new([System.Net.Http.HttpMethod]::Get, [string]$File.source.url)
    if ($existing -gt 0) {
        $request.Headers.Range = [System.Net.Http.Headers.RangeHeaderValue]::new($existing, $null)
    }

    $response = $null
    $networkStream = $null
    $output = $null
    try {
        $response = $client.SendAsync(
            $request,
            [System.Net.Http.HttpCompletionOption]::ResponseHeadersRead
        ).GetAwaiter().GetResult()
        if ($existing -gt 0 -and $response.StatusCode -ne [System.Net.HttpStatusCode]::PartialContent) {
            throw "Source rejected byte-range resume for '$($File.filename)'. The partial file was retained; discard it explicitly to restart."
        }
        $response.EnsureSuccessStatusCode()

        $responseLength = $response.Content.Headers.ContentLength
        $fileTotal = if ($File.expectedSizeBytes) {
            [long]$File.expectedSizeBytes
        } elseif ($null -ne $responseLength) {
            [long]$responseLength + $existing
        } else {
            $null
        }
        $effectiveOverallTotal = if ($null -ne $OverallTotal) {
            [long]$OverallTotal
        } elseif ($null -ne $fileTotal -and $Context.Files.Count -eq 1) {
            [long]$fileTotal
        } else {
            $null
        }

        $stage = $(if ($existing -gt 0) { 'download-resume' } else { 'download' })
        $null = Write-HermesModelDownloadProgress `
            -Context $Context `
            -Stage $stage `
            -Message "Transferring $($File.filename)." `
            -Status running `
            -BytesCompleted ($CompletedBefore + $existing) `
            -BytesTotal $effectiveOverallTotal `
            -Cancellable $true `
            -PauseSupported $true

        $networkStream = $response.Content.ReadAsStreamAsync().GetAwaiter().GetResult()
        $mode = $(if ($existing -gt 0) { [System.IO.FileMode]::Append } else { [System.IO.FileMode]::Create })
        $output = [System.IO.FileStream]::new(
            $File.partialPath,
            $mode,
            [System.IO.FileAccess]::Write,
            [System.IO.FileShare]::Read,
            1MB,
            [System.IO.FileOptions]::SequentialScan
        )
        $buffer = [byte[]]::new(1MB)
        $downloadedThisRun = [long]0
        $started = [System.Diagnostics.Stopwatch]::StartNew()
        $lastUpdate = [System.Diagnostics.Stopwatch]::StartNew()

        while ($true) {
            Assert-HermesModelDownloadNotControlled -Context $Context
            $read = $networkStream.ReadAsync($buffer, 0, $buffer.Length).GetAwaiter().GetResult()
            if ($read -le 0) {
                break
            }
            $output.WriteAsync($buffer, 0, $read).GetAwaiter().GetResult()
            $downloadedThisRun += $read

            if ($lastUpdate.ElapsedMilliseconds -ge 500) {
                $bytesForFile = $existing + $downloadedThisRun
                $overallCompleted = $CompletedBefore + $bytesForFile
                $rate = $(if ($started.Elapsed.TotalSeconds -gt 0) {
                    $downloadedThisRun / $started.Elapsed.TotalSeconds
                } else { 0.0 })
                $eta = if ($null -ne $effectiveOverallTotal -and $rate -gt 0) {
                    ([long]$effectiveOverallTotal - $overallCompleted) / $rate
                } else {
                    $null
                }
                $null = Write-HermesModelDownloadProgress `
                    -Context $Context `
                    -Stage $stage `
                    -Message "Transferring $($File.filename)." `
                    -Status running `
                    -BytesCompleted $overallCompleted `
                    -BytesTotal $effectiveOverallTotal `
                    -RateBytesPerSecond $rate `
                    -EtaSeconds $eta `
                    -Cancellable $true `
                    -PauseSupported $true `
                    -Counters @{ filesCompleted = 0; filesTotal = $Context.Files.Count }
                $lastUpdate.Restart()
            }
        }
        $output.Flush($true)
        return [long]($existing + $downloadedThisRun)
    } finally {
        if ($output) { $output.Dispose() }
        if ($networkStream) { $networkStream.Dispose() }
        if ($response) { $response.Dispose() }
        $request.Dispose()
        $client.Dispose()
        $handler.Dispose()
    }
}

function Test-HermesModelDownloadFile {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)] $Context,
        [Parameter(Mandatory)] $File,
        [Parameter(Mandatory)][int] $Index
    )

    if (-not (Test-Path -LiteralPath $File.partialPath -PathType Leaf)) {
        throw "Downloaded staging file is missing: $($File.filename)"
    }
    $item = Get-Item -LiteralPath $File.partialPath
    if ($File.expectedSizeBytes -and $item.Length -ne [long]$File.expectedSizeBytes) {
        throw "Downloaded size mismatch for '$($File.filename)': expected $($File.expectedSizeBytes), got $($item.Length)."
    }

    $stage = $(if ($Index -eq 0) { 'hash-verification' } else { 'auxiliary-file-verification' })
    $null = Write-HermesModelDownloadProgress `
        -Context $Context `
        -Stage $stage `
        -Message "Verifying $($File.filename)." `
        -Status running `
        -BytesCompleted (Get-HermesModelDownloadCompletedBytes -Context $Context) `
        -BytesTotal (Get-HermesModelDownloadExpectedTotal -Context $Context) `
        -Cancellable $true `
        -PauseSupported $false

    $actualHash = (Get-FileHash -LiteralPath $File.partialPath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($File.expectedSha256 -and $actualHash -ne $File.expectedSha256) {
        throw "SHA-256 mismatch for '$($File.filename)'."
    }
    $File.actualSha256 = $actualHash
    $File.actualSizeBytes = [long]$item.Length
}

function New-HermesModelDownloadManifest {
    [CmdletBinding()]
    param([Parameter(Mandatory)] $Context)

    $primary = $Context.Primary
    $metadata = [ordered]@{
        download = [ordered]@{
            taskId = $Context.TaskId
            completedAt = (Get-Date).ToUniversalTime().ToString('o')
            repository = $Context.Repository
            revision = $Context.Revision
            sourceIdentity = $Context.Source
        }
    }
    $auxiliary = @($Context.Files | Select-Object -Skip 1 | ForEach-Object {
        [ordered]@{
            kind = $_.kind
            filename = $_.filename
            localPath = $_.targetRelativePath
            source = $_.source.url
            sizeBytes = [long]$_.actualSizeBytes
            sha256 = $_.actualSha256
        }
    })
    if ($auxiliary.Count -gt 0) {
        $metadata.auxiliaryFiles = $auxiliary
    }

    return [ordered]@{
        schemaVersion = 1
        id = $Context.ModelId
        displayName = $Context.DisplayName
        alias = $Context.Alias
        repository = $Context.Repository
        revision = $Context.Revision
        filename = $Context.Filename
        localPath = $primary.targetRelativePath
        sizeBytes = [long]$primary.actualSizeBytes
        sha256 = $primary.actualSha256
        license = $(if ([string]::IsNullOrWhiteSpace($Context.License)) { $null } else { $Context.License })
        source = $Context.Source.url
        metadata = $metadata
        server = [ordered]@{
            jinja = $true
            extraArguments = @()
        }
    }
}
