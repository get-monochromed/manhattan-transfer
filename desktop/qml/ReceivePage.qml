import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.ScrollView {
    id: page
    required property var controller
    readonly property var requests: controller.transfers.filter(function(transfer) { return transfer.pending })
    clip: true
    contentWidth: availableWidth
    Controls.ScrollBar.horizontal.policy: Controls.ScrollBar.AlwaysOff

    ColumnLayout {
        width: page.availableWidth
        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing * 2
            spacing: Kirigami.Units.largeSpacing

            Surface {
                Layout.fillWidth: true
                implicitHeight: receiveColumn.implicitHeight + Kirigami.Units.largeSpacing * 4
                ColumnLayout {
                    id: receiveColumn
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.largeSpacing * 2
                    spacing: Kirigami.Units.largeSpacing
                    RowLayout {
                        Layout.fillWidth: true
                        Kirigami.Icon { source: "computer"; Layout.preferredWidth: 35; Layout.preferredHeight: 35 }
                        ColumnLayout {
                            Layout.fillWidth: true
                            Kirigami.Heading { Layout.fillWidth: true; level: 2; text: page.controller.deviceName || qsTr("This computer"); textFormat: Text.PlainText; elide: Text.ElideRight }
                            Controls.Label { Layout.fillWidth: true; text: qsTr("Find this name on the sending device"); wrapMode: Text.WordWrap; color: Kirigami.Theme.disabledTextColor }
                        }
                    }
                    Kirigami.Separator { Layout.fillWidth: true }
                    RowLayout {
                        Layout.fillWidth: true
                        ColumnLayout {
                            Layout.fillWidth: true
                            Controls.Label { text: qsTr("Quick Share"); font.bold: true }
                            Controls.Label {
                                Layout.fillWidth: true
                                wrapMode: Text.WordWrap
                                textFormat: Text.PlainText
                                text: page.controller.quickError || qsTr("Receive from Android’s built-in share menu.")
                                color: Kirigami.Theme.disabledTextColor
                            }
                        }
                        Controls.Switch {
                            text: qsTr("Visible")
                            enabled: page.controller.ready && !page.controller.quickError
                            checked: page.controller.quickVisible
                            onToggled: page.controller.setQuickVisible(checked)
                        }
                    }
                    Kirigami.Separator { Layout.fillWidth: true }
                    ColumnLayout {
                        Layout.fillWidth: true
                        RowLayout {
                            Layout.fillWidth: true
                            Controls.Label { Layout.fillWidth: true; text: qsTr("LocalSend"); font.bold: true }
                            Controls.Label { text: page.controller.localPort && !page.controller.localError ? qsTr("Ready") : qsTr("Unavailable"); color: Kirigami.Theme.disabledTextColor }
                        }
                        Controls.Label {
                            Layout.fillWidth: true
                            wrapMode: Text.Wrap
                            textFormat: Text.PlainText
                            text: page.controller.localError || qsTr("Open LocalSend on the sending device. This computer can receive while Manhattan Transfer is running.")
                            color: Kirigami.Theme.disabledTextColor
                        }
                    }
                }
            }

            Kirigami.Heading { level: 2; text: qsTr("Incoming requests"); Layout.fillWidth: true }
            Controls.Label {
                Layout.fillWidth: true
                visible: page.requests.length === 0
                text: qsTr("Waiting for a device to send something.")
                color: Kirigami.Theme.disabledTextColor
                wrapMode: Text.WordWrap
            }
            Repeater {
                model: page.requests
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
