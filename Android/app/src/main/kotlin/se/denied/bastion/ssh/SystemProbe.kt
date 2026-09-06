package se.denied.bastion.ssh

/** Agentless system snapshot collected with one read-only SSH command. */
data class SystemSnapshot(
    val hostname: String? = null,
    val os: String? = null,
    val kernel: String? = null,
    val cpuCount: Int? = null,
    val uptimeSeconds: Double? = null,
    val load: LoadAverage? = null,
    val memory: MemoryInfo? = null,
    val disks: List<DiskUsage> = emptyList(),
    val containers: List<DockerContainer> = emptyList(),
) {
    val rootDisk: DiskUsage? get() = disks.firstOrNull { it.mount == "/" }
}

data class LoadAverage(val one: Double, val five: Double, val fifteen: Double)

data class MemoryInfo(val totalBytes: Long, val availableBytes: Long) {
    val usedBytes: Long get() = (totalBytes - availableBytes).coerceAtLeast(0)
}

data class DiskUsage(
    val filesystem: String,
    val mount: String,
    val sizeBytes: Long,
    val usedBytes: Long,
    val availableBytes: Long,
    val capacityPercent: Int,
)

data class DockerContainer(val id: String, val name: String, val image: String, val status: String)

object SystemProbe {
    val command: String = listOf(
        "echo @@LOADAVG", "cat /proc/loadavg 2>/dev/null",
        "echo @@UPTIME", "cat /proc/uptime 2>/dev/null",
        "echo @@MEM", "cat /proc/meminfo 2>/dev/null",
        "echo @@DF", "df -kP 2>/dev/null",
        "echo @@OS", "cat /etc/os-release 2>/dev/null",
        "echo @@KERNEL", "uname -sr 2>/dev/null",
        "echo @@HOST", "cat /proc/sys/kernel/hostname 2>/dev/null",
        "echo @@NPROC", "nproc 2>/dev/null",
        "echo @@DOCKER", "docker ps --format '{{.ID}}|{{.Names}}|{{.Image}}|{{.Status}}' 2>/dev/null",
        "echo @@END",
    ).joinToString("; ")

    fun snapshot(session: BastionSshSession): SystemSnapshot = parse(session.run(command))

    fun parse(output: String): SystemSnapshot {
        val sections = mutableMapOf<String, MutableList<String>>()
        var current: String? = null
        output.lineSequence().forEach { raw ->
            val line = raw.removeSuffix("\r")
            if (line.startsWith("@@")) {
                current = line.removePrefix("@@")
                sections.getOrPut(current!!) { mutableListOf() }
            } else {
                current?.let { sections.getOrPut(it) { mutableListOf() }.add(line) }
            }
        }

        return SystemSnapshot(
            hostname = sections["HOST"]?.firstOrNull()?.trim()?.takeIf(String::isNotEmpty),
            os = parseOs(sections["OS"].orEmpty()),
            kernel = sections["KERNEL"]?.firstOrNull()?.trim()?.takeIf(String::isNotEmpty),
            cpuCount = sections["NPROC"]?.firstOrNull()?.trim()?.toIntOrNull(),
            uptimeSeconds = sections["UPTIME"]?.firstOrNull()?.trim()?.substringBefore(' ')?.toDoubleOrNull(),
            load = parseLoad(sections["LOADAVG"]?.firstOrNull()),
            memory = parseMemory(sections["MEM"].orEmpty()),
            disks = parseDisks(sections["DF"].orEmpty()),
            containers = parseDocker(sections["DOCKER"].orEmpty()),
        )
    }

    private fun parseLoad(line: String?): LoadAverage? {
        val values = line?.trim()?.split(Regex("\\s+"))?.mapNotNull(String::toDoubleOrNull).orEmpty()
        if (values.size < 3) return null
        return LoadAverage(values[0], values[1], values[2])
    }

    private fun parseMemory(lines: List<String>): MemoryInfo? {
        val kb = mutableMapOf<String, Long>()
        lines.forEach { line ->
            val fields = line.trim().split(Regex("\\s+"))
            if (fields.size >= 2) fields[1].toLongOrNull()?.let { kb[fields[0].removeSuffix(":")] = it }
        }
        val total = kb["MemTotal"] ?: return null
        val available = kb["MemAvailable"] ?: return null
        return MemoryInfo(total * 1024, available * 1024)
    }

    private fun parseDisks(lines: List<String>): List<DiskUsage> = lines.mapNotNull { line ->
        val fields = line.trim().split(Regex("\\s+"))
        if (fields.size < 6 || fields[0] == "Filesystem") return@mapNotNull null
        val blocks = fields[1].toLongOrNull() ?: return@mapNotNull null
        val used = fields[2].toLongOrNull() ?: return@mapNotNull null
        val available = fields[3].toLongOrNull() ?: return@mapNotNull null
        DiskUsage(
            filesystem = fields[0],
            mount = fields.drop(5).joinToString(" "),
            sizeBytes = blocks * 1024,
            usedBytes = used * 1024,
            availableBytes = available * 1024,
            capacityPercent = fields[4].removeSuffix("%").toIntOrNull() ?: 0,
        )
    }

    private fun parseOs(lines: List<String>): String? = lines
        .firstOrNull { it.startsWith("PRETTY_NAME=") }
        ?.substringAfter('=')
        ?.removeSurrounding("\"")
        ?.takeIf(String::isNotEmpty)

    private fun parseDocker(lines: List<String>): List<DockerContainer> = lines.mapNotNull { line ->
        val fields = line.split('|')
        if (fields.size < 4) return@mapNotNull null
        DockerContainer(fields[0], fields[1], fields[2], fields[3])
    }
}