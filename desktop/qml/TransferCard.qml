import QtQuick
import QtQuick.Controls as Controls
import QtQuick.Layouts
import org.kde.kirigami as Kirigami

Surface {
    id: card
    required property var transfer
    required property var controller
    readonly property bool incoming: transfer.kind === "inbound"
    readonly property string names: transfer.files || ""
    readonly property string preview: transfer.text || transfer.preview || ""
    implicitHeight: body.implicitHeight + Kirigami.Units.largeSpacing * 2

    ColumnLayout {
        id: body
        anchors.fill: parent
        anchors.margins: Kirigami.Units.largeSpacing
        spacing: Kirigami.Units.smallSpacing * 2

        RowLayout {
            Layout.fillWidth: true
            Kirigami.Icon { Layout.preferredWidth: 22; Layout.preferredHeight: 22; source: card.incoming ? "folder-downloads" : "document-send" }
            Controls.Label { Layout.fillWidth: true; text: card.transfer.device || qsTr("Nearby device"); textFormat: Text.PlainText; elide: Text.ElideRight; font.bold: true }
            Controls.Label { text: card.transfer.protocol; color: Kirigami.Theme.linkColor }
        }
        Controls.Label {
            Layout.fillWidth: true
            text: card.transfer.size ? qsTr("%1 · %2").arg(card.transfer.label).arg(card.transfer.size) : card.transfer.label
            wrapMode: Text.WordWrap
            color: Kirigami.Theme.disabledTextColor
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: card.names.length > 0
            text: card.names
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            maximumLineCount: 6
            elide: Text.ElideRight
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: card.preview.length > 0
            text: card.preview
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
            maximumLineCount: 5
            elide: Text.ElideRight
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: !card.transfer.final && (card.transfer.pin || "").length > 0
            text: qsTr("Verification code: %1").arg(card.transfer.pin || "")
            textFormat: Text.PlainText
        }
        Controls.Label {
            Layout.fillWidth: true
            visible: (card.transfer.reason || "").length > 0
            text: card.transfer.reason || ""
            textFormat: Text.PlainText
            wrapMode: Text.Wrap
        }
        Controls.ProgressBar {
            Layout.fillWidth: true
            visible: !card.transfer.final && !card.transfer.pending
            value: card.transfer.fraction
            indeterminate: card.transfer.state !== "SendingFiles" && card.transfer.state !== "ReceivingFiles"
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            Controls.Button { visible: card.transfer.pending; text: qsTr("Decline"); onClicked: card.controller.answer(card.transfer.id, false) }
            Controls.Button { visible: card.transfer.pending; text: qsTr("Accept"); highlighted: true; onClicked: card.controller.answer(card.transfer.id, true) }
            Controls.Button { visible: !card.transfer.final && !card.transfer.pending; text: qsTr("Cancel"); onClicked: card.controller.cancel(card.transfer.id) }
            Controls.Button { visible: card.transfer.retry; text: qsTr("Try with LocalSend"); onClicked: card.controller.retryLocal(card.transfer.id) }
            Controls.Button { visible: card.transfer.state === "Finished" && card.incoming && (card.transfer.saved_paths || "").length > 0; text: qsTr("Open downloads"); icon.name: "folder-open"; onClicked: card.controller.openDownloads() }
            Controls.Button { visible: card.transfer.state === "Finished" && card.incoming && (card.transfer.text || "").length > 0; text: qsTr("Copy text"); icon.name: "edit-copy"; onClicked: card.controller.copyText(card.transfer.text) }
        }
    }
}
