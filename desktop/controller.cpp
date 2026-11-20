#include "controller.h"

#include <QApplication>
#include <QClipboard>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusPendingCallWatcher>
#include <QDBusServiceWatcher>
#include <QDBusVariant>
#include <QDesktopServices>
#include <QDir>
#include <QFileInfo>
#include <QIcon>
#include <QJsonDocument>
#include <QLoggingCategory>
#include <QMenu>
#include <QSystemTrayIcon>
#include <QTimer>
#include <QUrl>
#include <QWindow>
#include <algorithm>

namespace {
const QString service = QStringLiteral("io.github.NearbyTransfer");
const QString path = QStringLiteral("/io/github/NearbyTransfer");
const QString quick = QStringLiteral("io.github.NearbyTransfer.QuickShare1");
const QString local = QStringLiteral("io.github.NearbyTransfer.LocalSend1");
const QString properties = QStringLiteral("org.freedesktop.DBus.Properties");
const QString notifications = QStringLiteral("org.freedesktop.Notifications");
const QString notificationsPath = QStringLiteral("/org/freedesktop/Notifications");

void closeNotification(const QString &owner, uint notificationId)
{
    auto message = QDBusMessage::createMethodCall(owner, notificationsPath, notifications, QStringLiteral("CloseNotification"));
    message << notificationId;
    QDBusConnection::sessionBus().asyncCall(message);
}

QString formattedSize(qulonglong size)
{
    return QLocale().formattedDataSize(size, 1);
}

QString protocolInterface(const QString &id)
{
    return id.startsWith(QStringLiteral("ls-")) ? local : quick;
}
}

Controller::Controller(QObject *parent) : QObject(parent)
{
    auto bus = QDBusConnection::sessionBus();
    bus.connect(service, path, quick, QStringLiteral("DeviceFound"), this, SLOT(onQuickDevice(QString,QVariantMap)));
    bus.connect(service, path, quick, QStringLiteral("DeviceLost"), this, SLOT(onQuickLost(QString)));
    bus.connect(service, path, quick, QStringLiteral("TransferUpdated"), this, SLOT(onTransfer(QString,QVariantMap)));
    bus.connect(notifications, notificationsPath, notifications, QStringLiteral("ActionInvoked"),
                this, SLOT(onNotificationAction(uint,QString,QDBusMessage)));
    bus.connect(notifications, notificationsPath, notifications, QStringLiteral("NotificationClosed"),
                this, SLOT(onNotificationClosed(uint,uint,QDBusMessage)));
    auto watcher = new QDBusServiceWatcher(service, bus, QDBusServiceWatcher::WatchForOwnerChange, this);
    connect(watcher, &QDBusServiceWatcher::serviceOwnerChanged, this,
            [this](const QString &, const QString &, const QString &owner) {
        if (owner.isEmpty()) {
            serviceGone(tr("The transfer service stopped."));
        } else {
            QTimer::singleShot(250, this, [this] { poll(true); });
        }
    });
    m_process.setProcessChannelMode(QProcess::ForwardedChannels);
    connect(&m_process, &QProcess::errorOccurred, this, [this](QProcess::ProcessError error) {
        if (error == QProcess::FailedToStart) {
            serviceGone(tr("Could not start the transfer service. %1").arg(m_process.errorString()));
        }
    });
    connect(&m_process, &QProcess::finished, this, [this](int code, QProcess::ExitStatus) {
        if (code != 0) {
            serviceGone(tr("The transfer service exited with code %1. Run the launcher from a terminal for details.").arg(code));
        }
    });
    auto timer = new QTimer(this);
    connect(timer, &QTimer::timeout, this, [this] { poll(); });
    timer->start(1500);
    QTimer::singleShot(0, this, &Controller::restartService);
}

Controller::~Controller()
{
    for (const auto &id : m_incomingNotices.keys()) {
        dismissIncomingNotice(id);
    }
    if (m_process.state() != QProcess::NotRunning) {
        m_process.terminate();
        if (!m_process.waitForFinished(2200)) {
            m_process.kill();
            m_process.waitForFinished(500);
        }
    }
}

void Controller::attachWindow(QWindow *window)
{
    m_window = window;
    if (qEnvironmentVariableIsSet("NEARBY_NO_TRAY")) {
        return;
    }
    m_tray = new QSystemTrayIcon(QIcon::fromTheme(QStringLiteral("document-share"), QIcon::fromTheme(QStringLiteral("folder-network"))), this);
    m_tray->setToolTip(tr("Manhattan Transfer"));
    auto *menu = new QMenu;
    connect(this, &QObject::destroyed, menu, &QMenu::deleteLater);
    menu->addAction(tr("Open Manhattan Transfer"), this, &Controller::showWindow);
    menu->addAction(tr("Open Downloads"), this, &Controller::openDownloads);
    menu->addSeparator();
    menu->addAction(tr("Quit"), qApp, &QCoreApplication::quit);
    m_tray->setContextMenu(menu);
    connect(m_tray, &QSystemTrayIcon::activated, this, [this](QSystemTrayIcon::ActivationReason reason) {
        if (reason == QSystemTrayIcon::Trigger || reason == QSystemTrayIcon::DoubleClick) {
            showWindow();
        }
    });
    connect(m_tray, &QSystemTrayIcon::messageClicked, this, [this] {
        showWindow();
        emit showReceive();
    });
    m_tray->show();
    QApplication::setQuitOnLastWindowClosed(false);
}

bool Controller::hideToTray()
{
    if (!m_tray || !m_window) {
        return false;
    }
    m_window->hide();
    return true;
}

void Controller::showWindow()
{
    if (m_window) {
        m_window->showNormal();
        m_window->raise();
        m_window->requestActivate();
    }
}

void Controller::Activate(const QStringList &paths)
{
    showWindow();
    if (!paths.isEmpty()) {
        queuePaths(paths);
        emit showSend();
    }
}

void Controller::restartService()
{
    if (m_process.state() != QProcess::NotRunning) {
        poll(true);
        return;
    }
    if (!QDBusConnection::sessionBus().isConnected()) {
        serviceGone(tr("The desktop session bus is unavailable."));
        return;
    }
    const auto registered = QDBusConnection::sessionBus().interface()->isServiceRegistered(service);
    if (registered.isValid() && registered.value()) {
        poll(true);
        return;
    }
    dismissBanner();
    auto executable = qEnvironmentVariable("NEARBY_SERVICE_BINARY");
    if (executable.isEmpty()) {
        executable = QCoreApplication::applicationDirPath() + QStringLiteral("/nearby-transfer-service");
    }
    m_process.start(executable, {});
}

void Controller::call(const QString &interface, const QString &method, const QVariantList &args, Reply onReply)
{
    auto message = QDBusMessage::createMethodCall(service, path, interface, method);
    message.setArguments(args);
    auto *watcher = new QDBusPendingCallWatcher(QDBusConnection::sessionBus().asyncCall(message, method == QStringLiteral("SendFiles") ? 45000 : 10000), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this,
            [watcher, onReply = std::move(onReply)] { onReply(watcher->reply()); watcher->deleteLater(); });
}

void Controller::action(const QString &interface, const QString &method, const QVariantList &args)
{
    call(interface, method, args, [this](const QDBusMessage &reply) {
        if (reply.type() == QDBusMessage::ErrorMessage) {
            showError(reply.errorMessage());
        }
    });
}

void Controller::getQuickVisibility()
{
    call(properties, QStringLiteral("Get"), {quick, QStringLiteral("Visible")}, [this](const QDBusMessage &reply) {
        if (reply.type() != QDBusMessage::ErrorMessage) {
            const bool visible = qvariant_cast<QDBusVariant>(reply.arguments().value(0)).variant().toBool();
            if (m_quickVisible != visible) {
                m_quickVisible = visible;
                emit statusChanged();
            }
        }
    });
}

void Controller::poll(bool initialise)
{
    if (m_polling || m_process.state() == QProcess::Starting) {
        return;
    }
    auto *busInterface = QDBusConnection::sessionBus().interface();
    if (!busInterface) {
        return;
    }
    const auto registered = busInterface->isServiceRegistered(service);
    if (!registered.isValid() || !registered.value()) {
        return;
    }
    m_polling = true;
    call(local, QStringLiteral("Status"), {}, [this, initialise](const QDBusMessage &reply) {
        m_polling = false;
        if (reply.type() == QDBusMessage::ErrorMessage) {
            return;
        }
        const auto status = QJsonDocument::fromJson(reply.arguments().value(0).toString().toUtf8()).toVariant().toMap();
        const bool fresh = !m_ready || initialise;
        const auto deviceName = status.value(QStringLiteral("name")).toString();
        const auto folder = status.value(QStringLiteral("folder")).toString();
        const auto localPort = status.value(QStringLiteral("port")).toInt();
        const auto localError = status.value(QStringLiteral("error")).toString();
        const auto discoveryError = status.value(QStringLiteral("discoveryError")).toString();
        const auto quickError = status.value(QStringLiteral("quickError")).toString();
        if (fresh || m_deviceName != deviceName || m_downloadFolder != folder || m_localPort != localPort
                || m_localError != localError || m_localDiscoveryError != discoveryError || m_quickError != quickError) {
            m_ready = true;
            m_deviceName = deviceName;
            m_downloadFolder = folder;
            m_localPort = localPort;
            m_localError = localError;
            m_localDiscoveryError = discoveryError;
            m_quickError = quickError;
            emit statusChanged();
        }
        if (fresh && m_quickError.isEmpty()) {
            action(quick, QStringLiteral("StartDiscovery"));
            getQuickVisibility();
        }
        call(local, QStringLiteral("Devices"), {}, [this](const QDBusMessage &answer) {
            if (answer.type() == QDBusMessage::ErrorMessage) {
                return;
            }
            auto list = QJsonDocument::fromJson(answer.arguments().value(0).toString().toUtf8()).toVariant().toList();
            std::sort(list.begin(), list.end(), [](const QVariant &a, const QVariant &b) {
                return a.toMap().value(QStringLiteral("id")).toString() < b.toMap().value(QStringLiteral("id")).toString();
            });
            if (list != m_localDevices) {
                m_localDevices = list;
                emit devicesChanged();
            }
        });
    });
}

void Controller::refresh()
{
    if (!m_ready || m_scanning) {
        return;
    }
    m_scanning = true;
    emit statusChanged();
    if (m_quickError.isEmpty()) {
        action(quick, QStringLiteral("StartDiscovery"));
    }
    if (m_localError.isEmpty()) {
        action(local, QStringLiteral("Refresh"));
    }
    QTimer::singleShot(1600, this, [this] {
        m_scanning = false;
        emit statusChanged();
        poll();
    });
}

void Controller::findAddress(const QString &address, uint port)
{
    call(local, QStringLiteral("FindAddress"), {address, port}, [this](const QDBusMessage &reply) {
        if (reply.type() == QDBusMessage::ErrorMessage) {
            showError(reply.errorMessage());
        } else {
            setPreferLocal(true);
            poll();
        }
    });
}

void Controller::setQuickVisible(bool visible)
{
    call(properties, QStringLiteral("Set"), {quick, QStringLiteral("Visible"), QVariant::fromValue(QDBusVariant(visible))},
         [this](const QDBusMessage &reply) {
        if (reply.type() == QDBusMessage::ErrorMessage) {
            showError(reply.errorMessage());
        }
        getQuickVisibility();
    });
}

void Controller::serviceGone(const QString &reason)
{
    ++m_serviceEpoch;
    m_sentBatches.clear();
    for (const auto &id : m_incomingNotices.keys()) {
        dismissIncomingNotice(id);
    }
    m_ready = false;
    m_scanning = false;
    m_quickDevices.clear();
    m_localDevices.clear();
    for (auto &transfer : m_transfers) {
        if (!transfer.value(QStringLiteral("final")).toBool()) {
            transfer[QStringLiteral("final")] = true;
            transfer[QStringLiteral("pending")] = false;
            transfer[QStringLiteral("label")] = tr("Interrupted");
        }
    }
    emit statusChanged();
    emit devicesChanged();
    emit transfersChanged();
    showError(reason);
}

void Controller::addUrls(const QList<QUrl> &urls)
{
    QStringList paths;
    for (const auto &url : urls) {
        if (url.isLocalFile()) {
            paths.append(url.toLocalFile());
        }
    }
    queuePaths(paths);
}

void Controller::queuePaths(const QStringList &paths)
{
    bool skipped = false;
    for (const auto &path : paths) {
        const QFileInfo file(path);
        if ((!file.isFile() && !file.isDir()) || !file.isReadable()) {
            skipped = true;
        } else if (!m_files.contains(file.absoluteFilePath())) {
            const auto absolutePath = file.absoluteFilePath();
            m_files.append(absolutePath);
            m_queueEntries.insert(absolutePath, ++m_nextQueueEntry);
        }
    }
    if (skipped) {
        showError(tr("Select readable files or folders."));
    }
    emit filesChanged();
}

QVariantList Controller::files() const
{
    QVariantList result;
    for (const auto &path : m_files) {
        const QFileInfo file(path);
        const bool isFolder = file.isDir();
        result.append(QVariantMap{{QStringLiteral("name"), file.fileName()},
            {QStringLiteral("size"), isFolder ? tr("Folder") : formattedSize(file.size())},
            {QStringLiteral("folder"), isFolder}, {QStringLiteral("path"), path}});
    }
    return result;
}

bool Controller::foldersQueued() const
{
    return std::any_of(m_files.cbegin(), m_files.cend(), [](const QString &path) {
        return QFileInfo(path).isDir();
    });
}

void Controller::removeFile(int index)
{
    if (index >= 0 && index < m_files.size()) {
        m_queueEntries.remove(m_files.takeAt(index));
        emit filesChanged();
    }
}

void Controller::clearFiles()
{
    m_files.clear();
    m_queueEntries.clear();
    emit filesChanged();
}

void Controller::onQuickDevice(const QString &id, const QVariantMap &details)
{
    auto info = details;
    info[QStringLiteral("id")] = id;
    info[QStringLiteral("type")] = details.value(QStringLiteral("device_type"));
    info[QStringLiteral("protocol")] = QStringLiteral("quick");
    m_quickDevices[id] = info;
    emit devicesChanged();
}

void Controller::onQuickLost(const QString &id)
{
    if (m_quickDevices.remove(id)) {
        emit devicesChanged();
    }
}

QVariantList Controller::devices() const
{
    QVariantList quickList;
    if (!m_preferLocal) {
        for (const auto &item : m_quickDevices) {
            quickList.append(item);
        }
    }
    QVariantList localList;
    for (const auto &item : m_localDevices) {
        auto device = item.toMap();
        device[QStringLiteral("protocol")] = QStringLiteral("local");
        localList.append(device);
    }
    const auto byName = [](const QVariant &a, const QVariant &b) {
        return a.toMap().value(QStringLiteral("name")).toString().localeAwareCompare(
               b.toMap().value(QStringLiteral("name")).toString()) < 0;
    };
    std::stable_sort(quickList.begin(), quickList.end(), byName);
    std::stable_sort(localList.begin(), localList.end(), byName);
    return quickList + localList;
}

void Controller::setPreferLocal(bool enabled)
{
    if (m_preferLocal != enabled) {
        m_preferLocal = enabled;
        emit preferenceChanged();
        emit devicesChanged();
    }
}

void Controller::send(const QString &protocol, const QString &id)
{
    if (!m_ready || m_files.isEmpty()) {
        return;
    }
    const bool viaQuick = protocol == QStringLiteral("quick");
    if (viaQuick && foldersQueued()) {
        showError(tr("Choose a LocalSend device to send folders."));
        return;
    }
    const auto batch = m_queueEntries;
    const auto epoch = m_serviceEpoch;
    if (viaQuick) {
        const auto row = m_transfers.value(id);
        if (!row.isEmpty() && !row.value(QStringLiteral("final")).toBool()) {
            showError(tr("A transfer to this device is already active."));
            return;
        }
        m_retryFiles[id] = m_files;
        m_sentBatches[id] = batch;
        QVariantMap starting{{QStringLiteral("state"), QStringLiteral("Starting")},
                             {QStringLiteral("kind"), QStringLiteral("outbound")},
                             {QStringLiteral("device"), m_quickDevices.value(id).value(QStringLiteral("name"))}};
        onTransfer(id, starting);
    }
    call(viaQuick ? quick : local, QStringLiteral("SendFiles"), {id, m_files},
        [this, id, viaQuick, batch, epoch](const QDBusMessage &reply) {
            if (epoch != m_serviceEpoch) {
                return;
            }
            if (reply.type() == QDBusMessage::ErrorMessage) {
                if (viaQuick) {
                    onTransfer(id, {{QStringLiteral("state"), QStringLiteral("Disconnected")},
                        {QStringLiteral("kind"), QStringLiteral("outbound")}, {QStringLiteral("reason"), reply.errorMessage()}});
                } else {
                    showError(reply.errorMessage());
                }
            } else if (!viaQuick) {
                const auto transferId = reply.arguments().value(0).toString();
                if (!transferId.isEmpty()) {
                    m_sentBatches[transferId] = batch;
                    settleSend(transferId);
                }
            }
        });
    emit showTransfers();
}

void Controller::retryLocal(const QString &id)
{
    if (m_retryFiles.contains(id)) {
        queuePaths(m_retryFiles.value(id));
    }
    setPreferLocal(true);
    emit showSend();
    refresh();
}

void Controller::answer(const QString &id, bool accept)
{
    const auto transfer = m_transfers.value(id);
    if (!transfer.value(QStringLiteral("pending")).toBool()) {
        return;
    }
    if (protocolInterface(id) == local) {
        action(local, QStringLiteral("Answer"), {id, accept});
    } else {
        action(quick, QStringLiteral("AnswerTransfer"), {id, transfer.value(QStringLiteral("consent")),
               accept ? QStringLiteral("accept") : QStringLiteral("decline")});
    }
}

void Controller::cancel(const QString &id)
{
    action(protocolInterface(id), QStringLiteral("Cancel"), {id});
}

bool Controller::isFinal(const QString &state)
{
    return state == QStringLiteral("Finished") || state == QStringLiteral("Rejected")
        || state == QStringLiteral("Cancelled") || state == QStringLiteral("Disconnected");
}

QString Controller::stateLabel(const QString &state, const QString &kind)
{
    if (state == QStringLiteral("WaitingForUserConsent")) return tr("Needs your approval");
    if (state == QStringLiteral("Finished")) return kind == QStringLiteral("inbound") ? tr("Received") : tr("Sent");
    if (state == QStringLiteral("Rejected")) return tr("Declined");
    if (state == QStringLiteral("Cancelled")) return tr("Cancelled");
    if (state == QStringLiteral("Disconnected")) return tr("Transfer failed");
    if (state == QStringLiteral("SendingFiles")) return tr("Sending");
    if (state == QStringLiteral("ReceivingFiles")) return tr("Receiving");
    if (state == QStringLiteral("SentIntroduction") || state == QStringLiteral("WaitingForResponse") || state == QStringLiteral("WaitingForRemoteConsent")) return tr("Waiting for the recipient");
    return tr("Connecting");
}

void Controller::onTransfer(const QString &id, const QVariantMap &details)
{
    const auto state = details.value(QStringLiteral("state")).toString();
    if (state.isEmpty()) {
        return;
    }
    const auto kind = details.value(QStringLiteral("kind")).toString();
    const bool pending = state == QStringLiteral("WaitingForUserConsent");
    auto item = m_transfers.value(id);
    const auto consent = details.value(QStringLiteral("consent")).toULongLong();
    const bool newRequest = pending && consent && (!item.value(QStringLiteral("pending")).toBool()
        || item.value(QStringLiteral("consent")).toULongLong() != consent);
    const bool restarting = (state == QStringLiteral("Starting") || newRequest) && item.value(QStringLiteral("final")).toBool();
    if (restarting || item.isEmpty()) {
        item.clear();
        m_transferOrder.removeAll(id);
        m_transferOrder.prepend(id);
    }
    for (auto it = details.cbegin(); it != details.cend(); ++it) {
        if (!it.value().toString().isEmpty() || !item.contains(it.key())) {
            item[it.key()] = it.value();
        }
    }
    item[QStringLiteral("id")] = id;
    item[QStringLiteral("protocol")] = protocolInterface(id) == local ? tr("LocalSend") : tr("Quick Share");
    item[QStringLiteral("label")] = stateLabel(state, kind);
    item[QStringLiteral("final")] = isFinal(state);
    item[QStringLiteral("pending")] = pending;
    item[QStringLiteral("retry")] = state == QStringLiteral("Disconnected") && kind == QStringLiteral("outbound") && m_retryFiles.contains(id);
    const auto total = item.value(QStringLiteral("total_bytes")).toULongLong();
    const auto done = item.value(QStringLiteral("ack_bytes")).toULongLong();
    item[QStringLiteral("fraction")] = total ? std::clamp(double(done) / double(total), 0.0, 1.0) : 0.0;
    item[QStringLiteral("size")] = total ? formattedSize(total) : QString();
    m_transfers[id] = item;
    if (!pending) {
        dismissIncomingNotice(id);
    }
    settleSend(id);
    // Retain completed transfers for this session while bounding stored rows.
    for (qsizetype index = m_transferOrder.size() - 1; index >= 50; --index) {
        const auto oldId = m_transferOrder.at(index);
        if (m_transfers.value(oldId).value(QStringLiteral("final")).toBool()) {
            m_transferOrder.removeAt(index);
            m_transfers.remove(oldId);
            m_retryFiles.remove(oldId);
        }
    }
    emit transfersChanged();
    if (newRequest) {
        showIncomingNotice(id, item);
    }
}

void Controller::settleSend(const QString &id)
{
    const auto transfer = m_transfers.value(id);
    if (!transfer.value(QStringLiteral("final")).toBool()
        || transfer.value(QStringLiteral("kind")).toString() != QStringLiteral("outbound")) {
        return;
    }
    const auto batch = m_sentBatches.take(id);
    if (transfer.value(QStringLiteral("state")).toString() != QStringLiteral("Finished")
        || !transfer.value(QStringLiteral("reason")).toString().isEmpty()) {
        return;
    }
    m_retryFiles.remove(id);
    auto removed = m_files.removeIf([this, &batch](const QString &path) {
        if (batch.value(path) != m_queueEntries.value(path)) {
            return false;
        }
        m_queueEntries.remove(path);
        return true;
    });
    if (removed) {
        emit filesChanged();
    }
}

void Controller::showIncomingNotice(const QString &id, const QVariantMap &transfer)
{
    dismissIncomingNotice(id);
    const auto bus = QDBusConnection::sessionBus();
    const auto server = bus.interface()->serviceOwner(notifications);
    if (!server.isValid() || server.value().isEmpty()) {
        if (m_window && m_window->isActive()) {
            emit showReceive();
        }
        return;
    }
    // Call the unique name so a restarted notification server cannot reuse this request's ID.
    const auto owner = server.value();
    const auto serial = ++m_nextNotice;
    m_incomingNotices.insert(id, IncomingNotice{serial, transfer.value(QStringLiteral("consent")).toULongLong(), 0, owner});
    const auto protocol = transfer.value(QStringLiteral("protocol")).toString();
    const auto sender = transfer.value(QStringLiteral("device")).toString().simplified().left(128);
    auto body = tr("%1 wants to share with you.").arg(sender.isEmpty() ? tr("A nearby device") : sender).toHtmlEscaped();
    const auto pin = transfer.value(QStringLiteral("pin")).toString();
    if (!pin.isEmpty()) {
        body += QStringLiteral("\n") + tr("Compare verification code %1 with the other device.").arg(pin).toHtmlEscaped();
    }
    const QStringList actions{QStringLiteral("default"), tr("Open"), QStringLiteral("accept"), tr("Accept"),
                              QStringLiteral("decline"), tr("Decline")};
    const QVariantMap hints{{QStringLiteral("desktop-entry"), QStringLiteral("io.github.NearbyTransfer")},
                            {QStringLiteral("resident"), true}};
    auto message = QDBusMessage::createMethodCall(owner, notificationsPath, notifications, QStringLiteral("Notify"));
    message.setArguments({tr("Manhattan Transfer"), uint(0), QStringLiteral("document-share"),
                          tr("Incoming %1 transfer").arg(protocol), body, actions, hints, 120000});
    auto *watcher = new QDBusPendingCallWatcher(bus.asyncCall(message, 5000), this);
    connect(watcher, &QDBusPendingCallWatcher::finished, this, [this, id, serial, owner, watcher] {
        const auto reply = watcher->reply();
        watcher->deleteLater();
        auto notice = m_incomingNotices.find(id);
        const bool current = notice != m_incomingNotices.end() && notice->serial == serial;
        if (reply.type() == QDBusMessage::ErrorMessage) {
            if (current) {
                m_incomingNotices.erase(notice);
                if (m_window && m_window->isActive()) {
                    emit showReceive();
                }
            }
            return;
        }
        const auto notificationId = reply.arguments().value(0).toUInt();
        if (current) {
            notice->notificationId = notificationId;
        } else if (notificationId) {
            closeNotification(owner, notificationId);
        }
    });
}

void Controller::dismissIncomingNotice(const QString &id)
{
    const auto notice = m_incomingNotices.take(id);
    if (notice.notificationId) {
        closeNotification(notice.owner, notice.notificationId);
    }
}

void Controller::onNotificationAction(uint notificationId, const QString &action, const QDBusMessage &signal)
{
    for (auto notice = m_incomingNotices.cbegin(); notice != m_incomingNotices.cend(); ++notice) {
        if (!notificationId || notice->notificationId != notificationId || notice->owner != signal.service()) {
            continue;
        }
        const auto id = notice.key();
        const auto current = m_transfers.value(id);
        if (!current.value(QStringLiteral("pending")).toBool()
            || current.value(QStringLiteral("consent")).toULongLong() != notice->consent) {
            return;
        }
        if (action == QStringLiteral("accept") || action == QStringLiteral("decline")) {
            answer(id, action == QStringLiteral("accept"));
        } else if (action == QStringLiteral("default")) {
            showWindow();
            emit showReceive();
        }
        return;
    }
}

void Controller::onNotificationClosed(uint notificationId, uint, const QDBusMessage &signal)
{
    for (auto notice = m_incomingNotices.begin(); notice != m_incomingNotices.end(); ++notice) {
        if (notificationId && notice->notificationId == notificationId && notice->owner == signal.service()) {
            m_incomingNotices.erase(notice);
            return;
        }
    }
}

QVariantList Controller::transfers() const
{
    QVariantList result;
    for (const auto &id : m_transferOrder) {
        result.append(m_transfers.value(id));
    }
    return result;
}

int Controller::pendingCount() const
{
    return int(std::count_if(m_transfers.cbegin(), m_transfers.cend(), [](const auto &row) {
        return row.value(QStringLiteral("pending")).toBool();
    }));
}

void Controller::openDownloads()
{
    if (!m_downloadFolder.isEmpty()) {
        QDesktopServices::openUrl(QUrl::fromLocalFile(m_downloadFolder));
    }
}

void Controller::copyText(const QString &text)
{
    QApplication::clipboard()->setText(text);
}

void Controller::showError(const QString &message)
{
    m_banner = message;
    emit bannerChanged();
}

void Controller::dismissBanner()
{
    m_banner.clear();
    emit bannerChanged();
}
