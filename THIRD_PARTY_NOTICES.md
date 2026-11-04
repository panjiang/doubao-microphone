# Third-party source notices

The application remains under its root MIT license. The UMDF research driver
under `research/windows-umdf/driver/` is adapted from Microsoft's vhidmini2
sample and is distributed in source form under the Microsoft Public License
(MS-PL), with the original notices retained. Its full license is included at
`research/windows-umdf/driver/LICENSE`. Unmodified reference copies are covered
by `research/windows-umdf/reference/LICENSE`; exact file hashes and URLs appear
in `research/windows-umdf/reference-sources.json`.

The original driver's Microsoft copyright notices remain in the adapted files.
This research does not imply Microsoft endorsement. Windows SDK/WDK tools are
build/test dependencies and are not included in the research source snapshot.
In particular, DevGen is not redistributed as a product installer.

HIDMaestro was consulted as architectural background. No HIDMaestro runtime,
source module or binary was incorporated in this prototype.


VB-CABLE Windows installer dependency
------------------------------------
The Windows installer includes the original, unmodified VB-CABLE Driver Pack 45
archive from VB-Audio / Vincent Burel. This proprietary dependency retains its
own end-user terms and is not covered by this application's MIT license.
Source: https://www.vb-cable.com/ (https://vb-audio.com/Cable/).
VB-CABLE is donationware; all participations are welcome.
Support or licenses: https://shop.vb-audio.com/.

The vendor's current published permission allows as-is bundling and silent
installation when its origin and donationware model remain visible. Additional
terms apply to professional/organizational deployment. See
https://vb-audio.com/Services/licensing.htm (reviewed 2026-11-03).
The older readme inside Pack 45 contains an integration-permission clause;
the current explicit published bundling permission is the basis for this use.
The complete original readme is displayed with the installer license notice.
The package version, hashes and verified signer identities are recorded in
packaging/vbcable.lock.json. Vendor binaries stay in the ignored build cache,
not the Git source tree. The application does not uninstall this shared driver.

BlackHole remains separately installed on macOS. No BlackHole binaries are
bundled by this change, and no change is made to this application's MIT license.
