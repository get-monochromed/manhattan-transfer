import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Kirigami.ApplicationWindow {
    id: window
    readonly property var controller: transferController
    property int section: 0
    title: qsTr("Manhattan Transfer")
    width: 720
    height: 725
    minimumWidth: 460
    minimumHeight: 470
    visible: false
    pageStack.globalToolBar.style: Kirigami.ApplicationHeaderStyle.None
    onClosing: function(event) { if (controller.hideToTray()) event.accepted = false }

    Connections {
        target: window.controller
        function onShowSend() { window.section = 0 }
        function onShowReceive() { window.section = 1 }
        function onShowTransfers() { window.section = 2 }
    }
    Shortcut { sequence: StandardKey.Quit; onActivated: Qt.quit() }
    Shortcut { sequence: StandardKey.Refresh; onActivated: window.controller.refresh() }
    Shortcut { sequence: StandardKey.Open; onActivated: { window.section = 0; sendPage.chooseFiles() } }

    pageStack.initialPage: Kirigami.Page {
        padding: 0

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            Controls.Pane {
                Layout.fillWidth: true
                padding: Kirigami.Units.largeSpacing * 2
                Kirigami.Theme.colorSet: Kirigami.Theme.Window

                contentItem: RowLayout {
                    spacing: Kirigami.Units.largeSpacing
                    Rectangle {
                        Layout.preferredWidth: 49
                        Layout.preferredHeight: 49
                        radius: Kirigami.Units.cornerRadius + 6
                        color: Qt.tint(Kirigami.Theme.backgroundColor, Qt.alpha(Kirigami.Theme.highlightColor, 0.17))
                        Kirigami.Icon { anchors.centerIn: parent; width: 28; height: 28; source: "document-share" }
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 2
                        Kirigami.Heading { level: 1; text: qsTr("Manhattan Transfer"); Layout.fillWidth: true; elide: Text.ElideRight }
                        Controls.Label {
                            Layout.fillWidth: true
                            text: window.controller.deviceName.length ? qsTr("Sharing as %1").arg(window.controller.deviceName) : qsTr("Nearby file sharing")
                            color: Kirigami.Theme.disabledTextColor
                            textFormat: Text.PlainText
                            elide: Text.ElideRight
                        }
                    }
                    Controls.ToolButton {
                        visible: window.section === 0
                        icon.name: "view-refresh"
                        text: qsTr("Refresh devices")
                        display: Controls.AbstractButton.IconOnly
                        enabled: window.controller.ready && !window.controller.scanning
                        onClicked: window.controller.refresh()
                        Controls.ToolTip.visible: hovered
                        Controls.ToolTip.text: text
                    }
                    Controls.ToolButton {
                        visible: window.section !== 0
                        icon.name: "folder-downloads"
                        text: qsTr("Open downloads")
                        display: Controls.AbstractButton.IconOnly
                        enabled: window.controller.ready
                        onClicked: window.controller.openDownloads()
                        Controls.ToolTip.visible: hovered
                        Controls.ToolTip.text: text
                    }
                }
            }

            Controls.TabBar {
                Layout.fillWidth: true
                Layout.leftMargin: Kirigami.Units.largeSpacing * 2
                Layout.rightMargin: Kirigami.Units.largeSpacing * 2
                currentIndex: window.section
                onCurrentIndexChanged: if (currentIndex !== window.section) window.section = currentIndex
                Controls.TabButton { text: qsTr("Send"); icon.name: "document-send" }
                Controls.TabButton {
                    text: window.controller.pendingCount > 0 ? qsTr("Receive (%1)").arg(window.controller.pendingCount) : qsTr("Receive")
                    icon.name: "folder-downloads"
                }
                Controls.TabButton { text: qsTr("Transfers"); icon.name: "view-list-details" }
            }

            Kirigami.InlineMessage {
                Layout.fillWidth: true
                Layout.margins: visible ? Kirigami.Units.largeSpacing * 2 : 0
                visible: window.controller.banner.length > 0
                type: Kirigami.MessageType.Error
                text: window.controller.banner.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;")
                showCloseButton: true
                onVisibleChanged: if (!visible && window.controller.banner.length > 0) window.controller.dismissBanner()
                actions: [Kirigami.Action {
                    text: qsTr("Restart service")
                    visible: !window.controller.ready
                    onTriggered: window.controller.restartService()
                }]
            }

            StackLayout {
                Layout.fillWidth: true
                Layout.fillHeight: true
                currentIndex: window.section
                SendPage { id: sendPage; controller: window.controller }
                ReceivePage { controller: window.controller }
                TransfersPage { controller: window.controller }
            }
        }
    }
}
