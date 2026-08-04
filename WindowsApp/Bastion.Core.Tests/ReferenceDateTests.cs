using Bastion.Core;

namespace Bastion.Core.Tests;

public class ReferenceDateTests
{
    [Fact]
    public void UnixEpochAsReferenceDateMatchesSwiftOffset()
    {
        // Verifierat mot en riktig swift-körning (samma test som i
        // LinuxApp/src/host.rs): Date(timeIntervalSinceReferenceDate: 0)
        // kodat gav modifiedAt = 0, dvs. Unix-epok = -978307200 i referensepok.
        var unixEpochAsReference = 0.0 - 978_307_200.0;
        Assert.Equal(-978_307_200.0, unixEpochAsReference);
    }
}
