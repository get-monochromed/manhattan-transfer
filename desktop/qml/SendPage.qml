import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Controls.ScrollView {
    id: page
    required property var controller
    property string selectedId: ""
    property string selectedProtocol: ""
    readonly property var visibleDevices: controller.foldersQueued
        ? controller.devices.filter(device => device.protocol === "local") : controller.devices
    readonly property var selected: visibleDevices.find(function(device) {
        return device.id === selectedId && device.protocol === selectedProtocol
    })
    function chooseFiles() { picker.open() }

    clip: true
    contentWidth: availableWidth
    Controls.ScrollBar.horizontal.policy: Controls.ScrollBar.AlwaysOff

    FileDialog {
        id: picker
        title: qsTr("Choose files to send")
        fileMode: FileDialog.OpenFiles
        onAccepted: page.controller.addUrls(selectedFiles)
    }

    FolderDialog {
        id: folderPicker
        title: qsTr("Choose a folder to send")
        onAccepted: page.controller.addUrls([selectedFolder])
    }

    Controls.Dialog {
        id: addressDialog
        title: qsTr("Find a LocalSend device")
        anchors.centerIn: parent
        width: Math.min(360, page.width - Kirigami.Units.largeSpacing * 4)
        modal: true
        standardButtons: Controls.Dialog.Ok | Controls.Dialog.Cancel
        onOpened: address.forceActiveFocus()
        onAccepted: page.controller.findAddress(address.text, port.value)
        ColumnLayout {
            anchors.fill: parent
            Controls.Label { text: qsTr("IP address"); Layout.fillWidth: true }
            Controls.TextField { id: address; placeholderText: "192.168.1.10"; Layout.fillWidth: true; Accessible.name: qsTr("IP address") }
            Controls.Label { text: qsTr("Port"); Layout.fillWidth: true }
            Controls.SpinBox { id: port; from: 1; to: 65535; value: 53317; editable: true; Layout.fillWidth: true; Accessible.name: qsTr("Port") }
        }
    }

    ColumnLayout {
        width: page.availableWidth
        spacing: Kirigami.Units.largeSpacing

        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing * 2
            Layout.bottomMargin: 0
            spacing: Kirigami.Units.largeSpacing

            Surface {
                Layout.fillWidth: true
                implicitHeight: 161
                color: drop.containsDrag
                    ? Qt.tint(Kirigami.Theme.backgroundColor, Qt.alpha(Kirigami.Theme.highlightColor, 0.18))
                    : Qt.tint(Kirigami.Theme.backgroundColor, Qt.alpha(Kirigami.Theme.highlightColor, 0.045))
                border.color: drop.containsDrag ? Kirigami.Theme.highlightColor : Kirigami.Theme.separatorColor
                DropArea {
                    id: drop
                    anchors.fill: parent
                    onDropped: function(event) { if (event.hasUrls) page.controller.addUrls(event.urls) }
                }
                ColumnLayout {
                    anchors.centerIn: parent
                    spacing: Kirigami.Units.smallSpacing * 2
                    Kirigami.Icon {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: 32
                        Layout.preferredHeight: 32
                        source: "folder-documents"
                    }
                    Kirigami.Heading {
                        level: 3
                        Layout.alignment: Qt.AlignHCenter
                        text: page.controller.files.length ? qsTr("Drop more files or folders here") : qsTr("Drop files or folders here")
                    }
                    RowLayout {
                        Layout.alignment: Qt.AlignHCenter
                        Controls.Button {
                            text: qsTr("Choose files…")
                            icon.name: "document-open"
                            onClicked: page.chooseFiles()
                        }
                        Controls.Button {
                            text: qsTr("Choose folder…")
                            icon.name: "folder-open"
                            onClicked: folderPicker.open()
                        }
                    }
                }
            }

            ColumnLayout {
                visible: page.controller.files.length > 0
                Layout.fillWidth: true
                spacing: 0
                RowLayout {
                    Layout.fillWidth: true
                    Controls.Label { text: page.controller.files.length === 1 ? qsTr("1 item selected") : qsTr("%1 items selected").arg(page.controller.files.length); Layout.fillWidth: true }
                    Controls.ToolButton { text: qsTr("Clear"); onClicked: page.controller.clearFiles() }
                }
                Repeater {
                    model: page.controller.files
                    delegate: RowLayout {
                        required property var modelData
                        required property int index
                        Layout.fillWidth: true
                        Kirigami.Icon { Layout.preferredWidth: 18; Layout.preferredHeight: 18; source: modelData.folder ? "folder" : "text-x-generic" }
                        Controls.Label { Layout.fillWidth: true; text: modelData.name; textFormat: Text.PlainText; elide: Text.ElideMiddle }
                        Controls.Label { text: modelData.size; color: Kirigami.Theme.disabledTextColor }
                        Controls.ToolButton {
                            text: qsTr("Remove %1").arg(modelData.name)
                            display: Controls.AbstractButton.IconOnly
                            icon.name: "list-remove"
                            onClicked: page.controller.removeFile(index)
                        }
                    }
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.margins: Kirigami.Units.largeSpacing * 2
            Layout.topMargin: Kirigami.Units.smallSpacing
            spacing: Kirigami.Units.largeSpacing

            RowLayout {
                Layout.fillWidth: true
                Kirigami.Heading { level: 2; text: qsTr("Nearby devices"); Layout.fillWidth: true }
                Controls.ComboBox {
                    model: [qsTr("Quick Share first"), qsTr("LocalSend only")]
                    enabled: !page.controller.foldersQueued
                    currentIndex: page.controller.preferLocal || page.controller.foldersQueued ? 1 : 0
                    onActivated: function(index) { page.controller.setPreferLocal(index === 1) }
                    Accessible.name: qsTr("Preferred protocol")
                }
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: page.controller.quickError.length > 0 && !page.controller.preferLocal
                type: Kirigami.MessageType.Warning
                text: qsTr("Quick Share is unavailable. You can still choose a LocalSend device.")
            }
            Kirigami.InlineMessage {
                Layout.fillWidth: true
                visible: page.controller.localError.length > 0 || page.controller.localDiscoveryError.length > 0
                type: Kirigami.MessageType.Warning
                text: page.controller.localError.length > 0 ? qsTr("LocalSend is unavailable. See Receive for details.")
                    : qsTr("LocalSend discovery is limited on this network.")
            }
            Controls.Label {
                Layout.fillWidth: true
                visible: page.controller.foldersQueued
                wrapMode: Text.WordWrap
                text: qsTr("Folders use LocalSend to preserve their structure.")
            }
            Surface {
                Layout.fillWidth: true
                implicitHeight: emptyContent.implicitHeight + Kirigami.Units.largeSpacing * 4
                visible: page.visibleDevices.length === 0
                ColumnLayout {
                    id: emptyContent
                    anchors.fill: parent
                    anchors.margins: Kirigami.Units.largeSpacing * 2
                    spacing: Kirigami.Units.smallSpacing * 2
                    Kirigami.Icon {
                        Layout.alignment: Qt.AlignHCenter
                        Layout.preferredWidth: 34
                        Layout.preferredHeight: 34
                        source: "network-wireless"
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        font.bold: true
                        text: !page.controller.ready ? qsTr("Starting the transfer service…") : page.controller.scanning ? qsTr("Looking for devices…") : qsTr("No devices found")
                    }
                    Controls.Label {
                        Layout.fillWidth: true
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        color: Kirigami.Theme.disabledTextColor
                        text: page.controller.preferLocal || page.controller.foldersQueued
                              ? qsTr("Open LocalSend on the other device and connect it to the same network.")
                              : qsTr("Open Quick Share in Receive mode or launch LocalSend on the other device. Keep both devices on the same Wi-Fi.")
                    }
                }
            }
            Repeater {
                model: page.visibleDevices
                delegate: Controls.ItemDelegate {
                    id: deviceRow
                    required property var modelData
                    Layout.fillWidth: true
                    padding: Kirigami.Units.largeSpacing
                    highlighted: page.selectedId === modelData.id && page.selectedProtocol === modelData.protocol
                    Accessible.name: qsTr("%1 via %2").arg(modelData.name).arg(modelData.protocol === "quick" ? qsTr("Quick Share") : qsTr("LocalSend"))
                    onClicked: { page.selectedId = modelData.id; page.selectedProtocol = modelData.protocol }
                    background: Surface {
                        color: deviceRow.highlighted || deviceRow.hovered
                               ? Qt.tint(Kirigami.Theme.backgroundColor, Qt.alpha(Kirigami.Theme.highlightColor, deviceRow.highlighted ? 0.19 : 0.07))
                               : Kirigami.Theme.backgroundColor
                        border.color: deviceRow.highlighted ? Kirigami.Theme.highlightColor : Kirigami.Theme.separatorColor
                    }
                    contentItem: RowLayout {
                        spacing: Kirigami.Units.largeSpacing
                        Kirigami.Icon {
                            Layout.preferredWidth: 27
                            Layout.preferredHeight: 27
                            source: /phone|mobile|tablet/i.test(deviceRow.modelData.type || "") ? "smartphone" : "computer"
                        }
                        ColumnLayout {
                            Layout.fillWidth: true
                            spacing: 2
                            Controls.Label { Layout.fillWidth: true; text: deviceRow.modelData.name || qsTr("Unnamed device"); textFormat: Text.PlainText; elide: Text.ElideRight; font.bold: true }
                            Controls.Label { text: deviceRow.modelData.ip || qsTr("Nearby device"); textFormat: Text.PlainText; color: Kirigami.Theme.disabledTextColor }
                        }
                        Controls.Label {
                            text: deviceRow.modelData.protocol === "quick" ? qsTr("Quick Share")
                                : deviceRow.modelData.encrypted ? qsTr("LocalSend") : qsTr("LocalSend · HTTP")
                            color: Kirigami.Theme.linkColor
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Controls.ToolButton {
                    text: qsTr("Enter LocalSend address…")
                    icon.name: "network-connect"
                    enabled: page.controller.ready && page.controller.localPort > 0
                    onClicked: addressDialog.open()
                }
                Item { Layout.fillWidth: true }
                Controls.Button {
                    visible: page.visibleDevices.length > 0
                    enabled: !!page.selected && page.controller.files.length > 0
                    text: page.selected ? qsTr("Send via %1").arg(page.selected.protocol === "quick" ? qsTr("Quick Share") : qsTr("LocalSend")) : qsTr("Send")
                    icon.name: "document-send"
                    highlighted: true
                    onClicked: page.controller.send(page.selected.protocol, page.selected.id)
                }
            }
        }
    }
}
