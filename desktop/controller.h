#pragma once

#include <QDBusMessage>
#include <QObject>
#include <QProcess>
#include <QUrl>
#include <QVariant>
#include <functional>

class QSystemTrayIcon;
class QWindow;

class Controller : public QObject
{
    Q_OBJECT
    Q_PROPERTY(QVariantList files READ files NOTIFY filesChanged)
    Q_PROPERTY(bool foldersQueued READ foldersQueued NOTIFY filesChanged)
    Q_PROPERTY(QVariantList devices READ devices NOTIFY devicesChanged)
    Q_PROPERTY(QVariantList transfers READ transfers NOTIFY transfersChanged)
    Q_PROPERTY(QString deviceName MEMBER m_deviceName NOTIFY statusChanged)
    Q_PROPERTY(QString downloadFolder MEMBER m_downloadFolder NOTIFY statusChanged)
    Q_PROPERTY(QString quickError MEMBER m_quickError NOTIFY statusChanged)
    Q_PROPERTY(QString localError MEMBER m_localError NOTIFY statusChanged)
    Q_PROPERTY(QString localDiscoveryError MEMBER m_localDiscoveryError NOTIFY statusChanged)
    Q_PROPERTY(bool quickVisible MEMBER m_quickVisible NOTIFY statusChanged)
    Q_PROPERTY(int localPort MEMBER m_localPort NOTIFY statusChanged)
    Q_PROPERTY(bool ready MEMBER m_ready NOTIFY statusChanged)
    Q_PROPERTY(bool scanning MEMBER m_scanning NOTIFY statusChanged)
    Q_PROPERTY(bool preferLocal MEMBER m_preferLocal NOTIFY preferenceChanged)
    Q_PROPERTY(QString banner MEMBER m_banner NOTIFY bannerChanged)
    Q_PROPERTY(int pendingCount READ pendingCount NOTIFY transfersChanged)

public:
    explicit Controller(QObject *parent = nullptr);
    ~Controller() override;

    QVariantList files() const;
    bool foldersQueued() const;
    QVariantList devices() const;
    QVariantList transfers() const;
    int pendingCount() const;
    void attachWindow(QWindow *window);
    void queuePaths(const QStringList &paths);

    Q_INVOKABLE void addUrls(const QList<QUrl> &urls);
    Q_INVOKABLE void removeFile(int index);
    Q_INVOKABLE void clearFiles();
    Q_INVOKABLE void setPreferLocal(bool enabled);
    Q_INVOKABLE void send(const QString &protocol, const QString &id);
    Q_INVOKABLE void retryLocal(const QString &id);
    Q_INVOKABLE void answer(const QString &id, bool accept);
    Q_INVOKABLE void cancel(const QString &id);
    Q_INVOKABLE void refresh();
    Q_INVOKABLE void findAddress(const QString &address, uint port);
    Q_INVOKABLE void setQuickVisible(bool visible);
    Q_INVOKABLE void openDownloads();
    Q_INVOKABLE void copyText(const QString &text);
    Q_INVOKABLE void dismissBanner();
    Q_INVOKABLE bool hideToTray();
    Q_INVOKABLE void restartService();

public slots:
    Q_SCRIPTABLE void Activate(const QStringList &paths);

signals:
    void filesChanged();
    void devicesChanged();
    void transfersChanged();
    void statusChanged();
    void bannerChanged();
    void preferenceChanged();
    void showReceive();
    void showSend();
    void showTransfers();

private slots:
    void onQuickDevice(const QString &id, const QVariantMap &details);
    void onQuickLost(const QString &id);
    void onTransfer(const QString &id, const QVariantMap &details);
    void onNotificationAction(uint notificationId, const QString &action, const QDBusMessage &signal);
    void onNotificationClosed(uint notificationId, uint reason, const QDBusMessage &signal);

private:
    using QueueSnapshot = QMap<QString, quint64>;
    struct IncomingNotice {
        quint64 serial;
        qulonglong consent;
        uint notificationId = 0;
        QString owner;
    };
    using Reply = std::function<void(const QDBusMessage &)>;
    void call(const QString &interface, const QString &method, const QVariantList &args, Reply onReply);
    void action(const QString &interface, const QString &method, const QVariantList &args = {});
    void getQuickVisibility();
    void showIncomingNotice(const QString &id, const QVariantMap &transfer);
    void dismissIncomingNotice(const QString &id);
    void settleSend(const QString &id);
    void poll(bool initialise = false);
    void serviceGone(const QString &reason);
    void showWindow();
    void showError(const QString &message);
    static bool isFinal(const QString &state);
    static QString stateLabel(const QString &state, const QString &kind);

    QProcess m_process;
    QWindow *m_window = nullptr;
    QSystemTrayIcon *m_tray = nullptr;
    QMap<QString, QVariantMap> m_quickDevices;
    QVariantList m_localDevices;
    QMap<QString, QVariantMap> m_transfers;
    QStringList m_transferOrder;
    QMap<QString, QStringList> m_retryFiles;
    QStringList m_files;
    QueueSnapshot m_queueEntries;
    QMap<QString, QueueSnapshot> m_sentBatches;
    QMap<QString, IncomingNotice> m_incomingNotices;
    quint64 m_nextQueueEntry = 0;
    quint64 m_nextNotice = 0;
    quint64 m_serviceEpoch = 0;
    QString m_deviceName;
    QString m_downloadFolder;
    QString m_quickError;
    QString m_localError;
    QString m_localDiscoveryError;
    QString m_banner;
    int m_localPort = 0;
    bool m_ready = false;
    bool m_quickVisible = false;
    bool m_preferLocal = false;
    bool m_scanning = false;
    bool m_polling = false;
};
