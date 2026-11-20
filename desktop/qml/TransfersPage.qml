import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.ScrollView {
    id: page
    required property var controller
    clip: true
    contentWidth: availableWidth
    Controls.ScrollBar.horizontal.policy: Controls.ScrollBar.AlwaysOff

    ColumnLayout {
        width: page.availableWidth
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing * 2
            spacing: Kirigami.Units.largeSpacing
            Kirigami.Heading { level: 2; text: qsTr("This session"); Layout.fillWidth: true }
            Controls.Label {
                Layout.fillWidth: true
                visible: page.controller.transfers.length === 0
                text: qsTr("Your sent and received files will appear here.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.WordWrap
            }
            Repeater {
                model: page.controller.transfers
                delegate: TransferCard {
                    required property var modelData
                    transfer: modelData
                    controller: page.controller
                    Layout.fillWidth: true
                }
            }
        }
    }
}
